use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const PREBUILT_CACHE_DIR: &str = ".cache/lbug-prebuilt";

fn link_mode() -> &'static str {
    if env::var("LBUG_SHARED").is_ok() {
        "dylib"
    } else {
        "static"
    }
}

fn get_target() -> String {
    env::var("PROFILE").unwrap()
}

fn link_openssl() {
    for var in ["OPENSSL_DIR", "OPENSSL_ROOT_DIR"] {
        if let Ok(dir) = env::var(var) {
            let path = PathBuf::from(&dir);
            let lib_dir = path.join("lib");
            let search = if lib_dir.is_dir() { lib_dir } else { path };
            println!("cargo:rustc-link-search=native={}", search.display());
            return;
        }
    }

    match vcpkg::find_package("openssl") {
        Ok(_) => return,
        Err(e) => println!("cargo:warning=vcpkg did not find openssl: {e}"),
    }

    if let Ok(output) = Command::new("pkg-config")
        .args(["--variable=libdir", "openssl"])
        .output()
    {
        if output.status.success() {
            let lib_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !lib_dir.is_empty() {
                let path = PathBuf::from(&lib_dir);
                if path.is_dir() {
                    println!("cargo:rustc-link-search=native={}", path.display());
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for prefix in ["/opt/homebrew/opt/openssl/lib", "/usr/local/opt/openssl/lib"] {
            let path = PathBuf::from(prefix);
            if path.is_dir() {
                println!("cargo:rustc-link-search=native={}", path.display());
                break;
            }
        }
    }

    #[cfg(not(windows))]
    {
        for dir in ["/usr/lib", "/usr/local/lib", "/usr/lib/x86_64-linux-gnu"] {
            let path = PathBuf::from(dir);
            if path.is_dir() {
                println!("cargo:rustc-link-search=native={}", path.display());
            }
        }
    }
}

fn link_libraries(link_bundled_deps: bool) {
    // This also needs to be set by any crates using it if they want to use extensions
    if !cfg!(windows) && link_mode() == "static" {
        println!("cargo:rustc-link-arg=-rdynamic");
    }
    if cfg!(windows) && link_mode() == "dylib" {
        println!("cargo:rustc-link-lib=dylib=lbug_shared");
    } else if link_mode() == "dylib" {
        println!("cargo:rustc-link-lib={}=lbug", link_mode());
    } else if rustversion::cfg!(since(1.82)) {
        println!("cargo:rustc-link-lib=static:+whole-archive=lbug");
    } else {
        println!("cargo:rustc-link-lib=static=lbug");
    }
    if link_mode() == "static" {
        if cfg!(windows) {
            println!("cargo:rustc-link-lib=dylib=msvcrt");
            println!("cargo:rustc-link-lib=dylib=shell32");
            println!("cargo:rustc-link-lib=dylib=ole32");
            println!("cargo:rustc-link-lib=dylib=advapi32");
            println!("cargo:rustc-link-lib=dylib=crypt32");
            println!("cargo:rustc-link-lib=dylib=user32");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
        } else if cfg!(target_os = "macos") {
            println!("cargo:rustc-link-lib=dylib=c++");
        } else {
            println!("cargo:rustc-link-lib=dylib=stdc++");
        }

        link_openssl();

        let (ssl_name, crypto_name) = if cfg!(windows) {
            ("libssl", "libcrypto")
        } else {
            ("ssl", "crypto")
        };
        println!("cargo:rustc-link-lib=dylib={ssl_name}");
        println!("cargo:rustc-link-lib=dylib={crypto_name}");

        if !link_bundled_deps {
            return;
        }

        for lib in [
            "utf8proc",
            "antlr4_cypher",
            "antlr4_runtime",
            "re2",
            "fastpfor",
            "parquet",
            "thrift",
            "snappy",
            "zstd",
            "miniz",
            "mbedtls",
            "brotlidec",
            "brotlicommon",
            "lz4",
            "roaring_bitmap",
            "simsimd",
            "yyjson",
        ] {
            if rustversion::cfg!(since(1.82)) {
                println!("cargo:rustc-link-lib=static:+whole-archive={lib}");
            } else {
                println!("cargo:rustc-link-lib=static={lib}");
            }
        }
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
}

fn static_lbug_file_name() -> &'static str {
    if cfg!(windows) {
        "lbug.lib"
    } else {
        "liblbug.a"
    }
}

fn prebuilt_cache_key() -> String {
    let source = if let Ok(run_id) = env::var("LBUG_PRECOMPILED_RUN_ID") {
        format!("run-{run_id}")
    } else if let Ok(version) = env::var("LBUG_VERSION") {
        format!("version-{version}")
    } else {
        "latest".to_string()
    };

    source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn prebuilt_lib_dir(manifest_dir: &Path) -> PathBuf {
    manifest_dir
        .join(PREBUILT_CACHE_DIR)
        .join(prebuilt_cache_key())
        .join("lib")
}

/// Global (default-visibility) symbol prefixes belonging to the vendored C
/// dependencies bundled into the prebuilt `liblbug.a` (zstd, lz4, simsimd,
/// `CRoaring`, mbedtls, yyjson, brotli). A consumer that independently links
/// another copy of any of these - e.g. simsimd via another embedded DB
/// engine, or zstd via a compression crate - hits duplicate-symbol errors,
/// made unavoidable on Rust >=1.82 where `+whole-archive` force-includes
/// every one of these regardless of whether lbug's own code reaches it.
/// `localize_vendored_symbols` below hides them from the link with
/// `objcopy --localize-symbol`, leaving only lbug's own C API (the
/// `lbug_`/`connection_`/`query_result_`/... names declared in
/// `include/lbug_rs.h` and `include/lbug_arrow.h`) globally visible.
///
/// Confirmed via `nm -g --defined-only` against the v0.18.0 prebuilt
/// archive that every one of these prefixes is actually present, that none
/// of them appear in `include/lbug_rs.h` or `include/lbug_arrow.h`, and that
/// after localization the only non-mangled global symbols left are the
/// `lbug_*` ones. `ZDICT_`/`HUF_`/`FSE_` (zstd's dictionary/entropy coders)
/// have zero current matches but are kept for a future liblbug build that
/// enables them - an unmatched `-w` glob is a no-op, not an error.
///
/// C++-mangled vendored symbols (the antlr4 runtime, httplib, the vendored
/// miniz/parquet reader) are intentionally not included here: some are
/// already renamed by upstream into an `lbug_`-prefixed C++ namespace
/// (`lbug_snappy::`, `lbug_parquet::`) for exactly this reason, and blindly
/// localizing raw mangled C++ names risks RTTI/typeinfo symbols that a
/// prefix-based glob can't reason about safely.
const VENDORED_SYMBOL_PREFIXES: &[&str] = &[
    // zstd
    "ZSTD_",
    "ZDICT_",
    "HUF_",
    "FSE_", //
    // lz4
    "LZ4_",
    "LZ4F_",
    "LZ4HC_", //
    // simsimd - the actual collider reproduced against another embedded
    // vector-search-capable DB engine sharing this binary
    "simsimd_", //
    // CRoaring: the public roaring_/roaring64_ API, its ART backend, and
    // the array/bitset/run container + array-util internals it links with
    // default visibility under their own operation names rather than a
    // shared library prefix
    "roaring_",
    "roaring64_",
    "ra_",
    "art_",
    "bitset_",
    "array_",
    "run_",
    "container_",
    "croaring_",
    "CROARING_",
    "intersect_",
    "intersection_",
    "union_",
    "xor_",
    "difference_",
    "convert_",
    "shared_container_",
    "bitsets_",
    "avx512_",
    "vbmi2_",
    "fast_union_",
    "extend_array",
    "align_size",
    "memequals",
    "interleavedBinarySearch",
    "binarySearch",
    "get_copy_of_container",
    "_avx2_",
    "_scalar_", //
    // mbedtls
    "mbedtls_", //
    // yyjson
    "yyjson_",
    "unsafe_yyjson_", //
    // brotli
    "Brotli",
    "kBrotli",
    "_kBrotli",
];

/// Cache-key-scoped location for the post-processed archive, kept beside
/// (not inside) `prebuilt_lib_dir` so the raw download it's derived from is
/// never overwritten and stays available as the fallback if processing
/// fails.
fn processed_lib_dir(manifest_dir: &Path) -> PathBuf {
    manifest_dir
        .join(PREBUILT_CACHE_DIR)
        .join(prebuilt_cache_key())
        .join("lib-processed")
}

fn run_step(cmd: &mut Command, step: &str) -> Result<(), String> {
    match cmd.output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(format!(
            "{step} exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Err(format!("{step} could not be run: {e}")),
    }
}

/// Merge the archive into one relocatable object (binding its internal
/// references first, so localizing a symbol below can never break a
/// reference between two of liblbug.a's own object files) and hide every
/// vendored prefix. This is the expensive step, so it only ever runs into
/// scratch space under `OUT_DIR`, once, regardless of where the result is
/// ultimately cached.
fn merge_and_localize(raw_archive: &Path, scratch_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(scratch_dir)
        .map_err(|e| format!("create scratch dir {}: {e}", scratch_dir.display()))?;

    let merged = scratch_dir.join("lbug-merged.o");
    run_step(
        Command::new("ld")
            .arg("-r")
            .arg("--whole-archive")
            .arg(raw_archive)
            .arg("-o")
            .arg(&merged),
        "ld -r --whole-archive",
    )?;

    let localized = scratch_dir.join("lbug-merged-localized.o");
    let mut objcopy = Command::new("objcopy");
    objcopy.arg("-w");
    for prefix in VENDORED_SYMBOL_PREFIXES {
        objcopy.arg(format!("--localize-symbol={prefix}*"));
    }
    objcopy.arg(&merged).arg(&localized);
    run_step(&mut objcopy, "objcopy --localize-symbol")?;

    Ok(localized)
}

/// Archive `localized_obj` into `dest_dir/liblbug.a`, writing under a
/// temporary name first so a build killed mid-`ar` can never leave a
/// truncated archive that a later build's cache-hit check mistakes for a
/// complete one.
fn archive_into(localized_obj: &Path, dest_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("create {}: {e}", dest_dir.display()))?;

    let final_path = dest_dir.join(static_lbug_file_name());
    let tmp_path = dest_dir.join(format!(
        "{}.tmp-{}",
        static_lbug_file_name(),
        std::process::id()
    ));
    run_step(
        Command::new("ar")
            .arg("rcs")
            .arg(&tmp_path)
            .arg(localized_obj),
        "ar rcs",
    )?;
    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| format!("rename processed archive into place: {e}"))?;
    Ok(())
}

/// Post-process the prebuilt static archive so its vendored dependencies
/// are hidden from the link (see `VENDORED_SYMBOL_PREFIXES`), returning the
/// directory to link against instead of `raw_lib_dir` on success.
///
/// Runs once per cache key - a processed archive already at the cache path
/// is reused as-is. If the toolchain this is proven on isn't the one in
/// use, or `ld`/`objcopy`/`ar` are missing, or any step fails, this falls
/// back to `None` (the caller links the raw, unprocessed archive) rather
/// than breaking a build that works today; a `cargo:warning` says what was
/// skipped and why.
fn localize_vendored_symbols(manifest_dir: &Path, raw_lib_dir: &Path) -> Option<PathBuf> {
    // Proven on linux-gnu only: `ld -r --whole-archive` / `objcopy -w
    // --localize-symbol` is a GNU binutils flow with no direct equivalent
    // on macOS (Mach-O `ld -r` takes different flags and no
    // --localize-symbol) or the MSVC toolchain.
    if !(cfg!(target_os = "linux") && cfg!(target_env = "gnu")) {
        return None;
    }

    let preferred_dir = processed_lib_dir(manifest_dir);
    if preferred_dir.join(static_lbug_file_name()).exists() {
        return Some(preferred_dir);
    }

    let out_dir = env::var_os("OUT_DIR")?;
    let scratch = PathBuf::from(&out_dir).join("lbug-vendored-symbol-localization");
    let raw_archive = raw_lib_dir.join(static_lbug_file_name());

    let localized_obj = match merge_and_localize(&raw_archive, &scratch) {
        Ok(p) => p,
        Err(e) => {
            println!(
                "cargo:warning=Could not localize vendored symbols in liblbug.a ({e}); linking the \
                 unprocessed archive - its bundled vendored dependencies (zstd, simsimd, ...) keep \
                 default GLOBAL visibility and may duplicate-symbol-conflict with another copy of the \
                 same dependency elsewhere in the link"
            );
            return None;
        }
    };

    // manifest_dir/.cache is where try_download_prebuilt_lbug already wrote
    // the raw archive we just processed, so it's normally writable; a
    // read-only mount after a warm-cache step (seen in some CI layouts) is
    // the only case that should land here, and OUT_DIR - always writable,
    // scoped to this build - is the fallback rather than failing the build
    // over a cache location that turned out read-only.
    for dest_dir in [
        preferred_dir,
        PathBuf::from(&out_dir).join("lbug-prebuilt-processed"),
    ] {
        match archive_into(&localized_obj, &dest_dir) {
            Ok(()) => return Some(dest_dir),
            Err(e) => println!(
                "cargo:warning=Could not write localized liblbug.a to {} ({e}); trying fallback location",
                dest_dir.display()
            ),
        }
    }

    println!(
        "cargo:warning=Could not write the localized archive anywhere writable; linking the unprocessed \
         archive with globally visible vendored symbols"
    );
    None
}

fn prebuilt_source_desc() -> String {
    let repo =
        env::var("LBUG_GITHUB_REPOSITORY").unwrap_or_else(|_| "LadybugDB/ladybug".to_string());
    if let Ok(run_id) = env::var("LBUG_PRECOMPILED_RUN_ID") {
        format!("run:{repo}/{run_id}")
    } else if let Ok(version) = env::var("LBUG_VERSION") {
        let version = version.strip_prefix('v').unwrap_or(&version);
        format!("release:{repo}/v{version}")
    } else {
        format!("release:{repo}/latest")
    }
}

fn emit_lbug_metadata(source: &str, lib_dir: &Path) {
    println!("cargo:rustc-env=LBUG_PRECOMPILED_SOURCE={source}");
    println!(
        "cargo:rustc-env=LBUG_PRECOMPILED_LIBRARY_DIR={}",
        lib_dir.display()
    );
}

fn try_download_prebuilt_lbug(manifest_dir: &Path) -> bool {
    for var in [
        "LBUG_PRECOMPILED_RUN_ID",
        "LBUG_VERSION",
        "LBUG_GITHUB_REPOSITORY",
        "LBUG_LINUX_VARIANT",
        "LBUG_LIB_KIND",
        "LBUG_BUILD_FROM_SOURCE",
        "LBUG_RUST_BUILD_FROM_SOURCE",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    if link_mode() != "static" {
        return false;
    }
    if env::var("LBUG_BUILD_FROM_SOURCE").is_ok() || env::var("LBUG_RUST_BUILD_FROM_SOURCE").is_ok()
    {
        println!("cargo:warning=Skipping prebuilt liblbug because source build was requested");
        return false;
    }

    let lib_dir = prebuilt_lib_dir(manifest_dir);
    let lib_path = lib_dir.join(static_lbug_file_name());
    if lib_path.exists() {
        return true;
    }

    let sh_script = manifest_dir.join("scripts").join("download_lbug.sh");
    let ps_script = manifest_dir.join("scripts").join("download_lbug.ps1");

    if sh_script.exists() {
        let status = Command::new("sh")
            .arg(&sh_script)
            .env("LBUG_TARGET_DIR", &lib_dir)
            .current_dir(manifest_dir)
            .status();

        match status {
            Ok(s) if s.success() && lib_path.exists() => return true,
            Ok(s) => println!(
                "cargo:warning=Prebuilt liblbug download failed with status {s}; building from source"
            ),
            Err(e) => println!(
                "cargo:warning=Could not run prebuilt liblbug downloader ({e}); building from source"
            ),
        }
    }

    if cfg!(windows) && ps_script.exists() {
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(&ps_script)
            .env("LBUG_TARGET_DIR", &lib_dir)
            .current_dir(manifest_dir)
            .status();

        match status {
            Ok(s) if s.success() && lib_path.exists() => return true,
            Ok(s) => println!(
                "cargo:warning=Prebuilt liblbug download failed with status {s}; building from source"
            ),
            Err(e) => println!(
                "cargo:warning=Could not run prebuilt liblbug downloader ({e}); building from source"
            ),
        }
    }

    false
}

fn use_prebuilt_lbug(manifest_dir: &Path) -> Option<Vec<PathBuf>> {
    if !try_download_prebuilt_lbug(manifest_dir) {
        return None;
    }

    let lib_dir = prebuilt_lib_dir(manifest_dir);
    let link_dir =
        localize_vendored_symbols(manifest_dir, &lib_dir).unwrap_or_else(|| lib_dir.clone());
    println!("cargo:rustc-link-search=native={}", link_dir.display());
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    emit_lbug_metadata(&prebuilt_source_desc(), &lib_dir);
    Some(vec![lib_dir])
}

fn get_lbug_root() -> PathBuf {
    let manifest_dir = manifest_dir();
    if let Ok(lbug_source_dir) = std::env::var("LBUG_SOURCE_DIR") {
        let root = PathBuf::from(lbug_source_dir);
        if root.is_symlink() || root.is_dir() {
            return root;
        }
    }

    let sibling_root = manifest_dir.join("../ladybug");
    if sibling_root.is_symlink() || sibling_root.is_dir() {
        return sibling_root;
    }

    let bundled_root = manifest_dir.join("lbug-src");
    if bundled_root.is_symlink() || bundled_root.is_dir() {
        return bundled_root;
    }
    if cfg!(windows) {
        let in_source_root = manifest_dir.join("../..");
        if in_source_root.join("CMakeLists.txt").exists() {
            return in_source_root;
        }
    }

    let lbug_dir = manifest_dir.join("lbug-src");
    if !lbug_dir.exists() {
        let version = std::env::var("LBUG_VERSION").unwrap_or_else(|_| "main".to_string());
        println!("Downloading ladybug source version {version}...");
        let url = if version.starts_with('v') {
            format!(
                "https://github.com/LadybugDB/ladybug/archive/refs/tags/{}.tar.gz",
                version
            )
        } else if version == "main" {
            "https://github.com/LadybugDB/ladybug/archive/refs/heads/main.tar.gz".to_string()
        } else {
            format!(
                "https://github.com/LadybugDB/ladybug/archive/refs/tags/v{}.tar.gz",
                version
            )
        };

        let output = std::process::Command::new("curl")
            .args(["-sL", &url])
            .arg("-o")
            .arg("ladybug.tar.gz")
            .current_dir(&manifest_dir)
            .output()
            .expect("Failed to download ladybug source");

        if !output.status.success() {
            panic!("Failed to download ladybug source from {}", url);
        }

        std::fs::create_dir_all(&lbug_dir).expect("Failed to create lbug-src directory");
        std::process::Command::new("tar")
            .args([
                "-xzf",
                "ladybug.tar.gz",
                "--strip-components=1",
                "-C",
                "lbug-src",
            ])
            .current_dir(&manifest_dir)
            .status()
            .expect("Failed to extract ladybug source");

        std::fs::remove_file(manifest_dir.join("ladybug.tar.gz")).ok();
    }

    lbug_dir
}

fn build_bundled_cmake() -> Vec<PathBuf> {
    let lbug_root = get_lbug_root();

    let mut build = cmake::Config::new(&lbug_root);
    build
        .no_build_target(true)
        .define("BUILD_SHELL", "OFF")
        .define("BUILD_SINGLE_FILE_HEADER", "OFF")
        .define("AUTO_UPDATE_GRAMMAR", "OFF");
    if cfg!(windows) {
        if Command::new("ninja")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            build.generator("Ninja");
        }
        build.cxxflag("/EHsc");
        build.define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL");
        build.define("CMAKE_POLICY_DEFAULT_CMP0091", "NEW");
    }
    if let Ok(jobs) = std::env::var("NUM_JOBS") {
        std::env::set_var("CMAKE_BUILD_PARALLEL_LEVEL", jobs);
    }
    let build_dir = build.build();

    let lbug_lib_path = build_dir.join("build").join("src");
    println!("cargo:rustc-link-search=native={}", lbug_lib_path.display());

    vec![
        lbug_root.join("src/include"),
        build_dir.join("build/src"),
        build_dir.join("build/src/include"),
        lbug_root.join("third_party/nlohmann_json"),
        lbug_root.join("third_party/fastpfor"),
        lbug_root.join("third_party/alp/include"),
    ]
}

fn build_ffi(
    bridge_file: &str,
    out_name: &str,
    source_file: &str,
    bundled: bool,
    include_paths: &Vec<PathBuf>,
) {
    let mut build = cxx_build::bridge(bridge_file);
    build.file(source_file);

    if bundled {
        build.define("LBUG_BUNDLED", None);
    }
    if get_target() == "debug" || get_target() == "relwithdebinfo" {
        build.define("ENABLE_RUNTIME_CHECKS", "1");
    }
    if link_mode() == "static" {
        build.define("LBUG_STATIC_DEFINE", None);
    }
    build.includes(include_paths);

    println!("cargo:rerun-if-env-changed=LBUG_SHARED");

    println!("cargo:rerun-if-changed=include/lbug_rs.h");
    println!("cargo:rerun-if-changed=src/lbug_rs.cpp");
    println!("cargo:rerun-if-changed={bridge_file}");
    println!("cargo:rerun-if-changed={source_file}");
    if cfg!(feature = "arrow") {
        println!("cargo:rerun-if-changed=include/lbug_arrow.h");
    }
    if bundled {
        // Note that this should match the lbug-src/* entries in the package.include list in Cargo.toml
        // Unfortunately they appear to need to be specified individually since the symlink is
        // considered to be changed each time.
        println!("cargo:rerun-if-changed=lbug-src/src");
        println!("cargo:rerun-if-changed=lbug-src/cmake");
        println!("cargo:rerun-if-changed=lbug-src/third_party");
        println!("cargo:rerun-if-changed=lbug-src/CMakeLists.txt");
        println!("cargo:rerun-if-changed=lbug-src/tools/CMakeLists.txt");
    }

    if cfg!(windows) {
        build.flag("/std:c++20");
        build.flag("/MD");
    } else {
        build.flag("-std=c++2a");
    }
    build.compile(out_name);
}

fn main() {
    if env::var("DOCS_RS").is_ok() {
        // Do nothing; we're just building docs and don't need the C++ library
        return;
    }

    let manifest_dir = manifest_dir();
    let mut bundled = false;
    let link_bundled_deps = false;
    let mut include_paths = vec![manifest_dir.join("include")];

    if let (Ok(lbug_lib_dir), Ok(lbug_include)) =
        (env::var("LBUG_LIBRARY_DIR"), env::var("LBUG_INCLUDE_DIR"))
    {
        println!("cargo:rustc-link-search=native={lbug_lib_dir}");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{lbug_lib_dir}");
        emit_lbug_metadata("external", Path::new(&lbug_lib_dir));
        include_paths.push(Path::new(&lbug_include).to_path_buf());
    } else if let Some(prebuilt_include_paths) = use_prebuilt_lbug(&manifest_dir) {
        include_paths.extend(prebuilt_include_paths);
    } else {
        include_paths.extend(build_bundled_cmake());
        bundled = true;
        println!("cargo:rustc-env=LBUG_PRECOMPILED_SOURCE=source");
        println!("cargo:rustc-env=LBUG_PRECOMPILED_LIBRARY_DIR=");
    }
    if link_mode() == "static" {
        link_libraries(link_bundled_deps);
    }
    build_ffi(
        "src/ffi.rs",
        "lbug_rs",
        "src/lbug_rs.cpp",
        bundled,
        &include_paths,
    );

    if cfg!(feature = "arrow") {
        build_ffi(
            "src/ffi/arrow.rs",
            "lbug_arrow_rs",
            "src/lbug_arrow.cpp",
            bundled,
            &include_paths,
        );
    }
    if link_mode() == "dylib" {
        link_libraries(link_bundled_deps);
    }
}
