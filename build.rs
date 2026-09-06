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
        for prefix in [
            "/opt/homebrew/opt/openssl/lib",
            "/usr/local/opt/openssl/lib",
        ] {
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

/// Where the downloaded prebuilt archive lives.
///
/// `LBUG_PREBUILT_CACHE_DIR` names a persistent cache shared across builds and
/// projects. Otherwise the archive goes under this build's `OUT_DIR`, which
/// Cargo owns: a registry checkout under `~/.cargo/registry/src` is meant to be
/// immutable, and writing the archive there broke `cargo vendor`, read-only
/// registries and offline builds. An archive already present in the legacy
/// in-tree cache is still used, so existing checkouts do not download again.
fn prebuilt_lib_dir(manifest_dir: &Path) -> PathBuf {
    let key = prebuilt_cache_key();
    let legacy = manifest_dir.join(PREBUILT_CACHE_DIR).join(&key).join("lib");
    if legacy.join(static_lbug_file_name()).exists() {
        return legacy;
    }
    let root = env::var_os("LBUG_PREBUILT_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| out_dir().join("lbug-prebuilt"));
    root.join(key).join("lib")
}

fn out_dir() -> PathBuf {
    PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for build scripts"))
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
        "LBUG_PREBUILT_CACHE_DIR",
        "LBUG_LOCALIZE_BUNDLED_SYMBOLS",
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

    // The downloaded directory holds the headers as well as the archive; a
    // localized archive lives elsewhere but the headers stay where they are.
    let lib_dir = prebuilt_lib_dir(manifest_dir);
    let link_dir = localize_bundled_symbols(&lib_dir).unwrap_or_else(|| lib_dir.clone());
    println!("cargo:rustc-link-search=native={}", link_dir.display());
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    emit_lbug_metadata(&prebuilt_source_desc(), &link_dir);
    Some(vec![lib_dir])
}

/// Hide the third-party C symbols the prebuilt archive carries, on request.
///
/// `liblbug.a` bundles zstd, lz4, brotli, simsimd, yyjson and CRoaring with
/// their ordinary exported names (`ZSTD_compressBound`, `roaring_bitmap_add`,
/// even bare helpers such as `get` and `union`). A binary that also links the
/// Rust crates for those libraries (`zstd-sys`, `lz4-sys`, `simsimd`,
/// `croaring-sys`, …) fails to link on Linux with `rust-lld`: duplicate
/// symbol, twenty times over, whereas macOS `ld64` silently keeps whichever
/// copy comes first.
///
/// With `LBUG_LOCALIZE_BUNDLED_SYMBOLS=1` on an ELF target the archive is
/// partially linked into one relocatable object, every unmangled global symbol
/// except the `lbug_*` C API is made local, and the result is archived back
/// under `OUT_DIR`. Internal references resolve inside that object, the C++ API
/// (mangled `lbug::` symbols) and the C API stay global, and the bundled
/// libraries no longer collide with anyone else's copy. It is opt-in because a
/// dynamically loaded extension that expects to resolve one of those bundled
/// symbols from the host binary would stop finding it; the maintainers can
/// flip the default once extensions are checked. Needs `ld`, `objcopy`, `nm`
/// and `ar` (GNU binutils); without them the archive is used as downloaded.
fn localize_bundled_symbols(lib_dir: &Path) -> Option<PathBuf> {
    let requested = env::var("LBUG_LOCALIZE_BUNDLED_SYMBOLS")
        .map(|value| value != "0" && !value.is_empty())
        .unwrap_or(false);
    if !requested || !cfg!(target_os = "linux") || link_mode() != "static" {
        return None;
    }
    let archive = lib_dir.join(static_lbug_file_name());
    let work = out_dir().join("lbug-localized");
    let out_lib_dir = work.join("lib");
    let localized = out_lib_dir.join(static_lbug_file_name());
    if localized.exists() {
        return Some(out_lib_dir);
    }
    if let Err(error) = std::fs::create_dir_all(&out_lib_dir) {
        println!(
            "cargo:warning=Cannot create {}: {error}; linking liblbug as downloaded",
            work.display()
        );
        return None;
    }
    let merged = work.join("lbug-merged.o");
    let keep = work.join("keep-global.txt");
    let steps: [(&str, Vec<std::ffi::OsString>); 3] = [
        (
            "ld",
            vec![
                "-r".into(),
                "--whole-archive".into(),
                archive.clone().into(),
                "--no-whole-archive".into(),
                "-o".into(),
                merged.clone().into(),
            ],
        ),
        (
            "nm",
            vec![
                "--defined-only".into(),
                "-g".into(),
                "--format=posix".into(),
                merged.clone().into(),
            ],
        ),
        (
            "ar",
            vec![
                "rcs".into(),
                localized.clone().into(),
                merged.clone().into(),
            ],
        ),
    ];
    for (tool, args) in &steps {
        let output = match Command::new(tool).args(args).output() {
            Ok(output) => output,
            Err(error) => {
                println!(
                    "cargo:warning=Cannot run {tool} ({error}); linking liblbug as downloaded"
                );
                return None;
            }
        };
        if !output.status.success() {
            println!("cargo:warning={tool} failed while localizing liblbug symbols; linking it as downloaded");
            return None;
        }
        if *tool == "nm" {
            // Localize only strong, unmangled symbols outside the C API: the
            // bundled libraries' functions and data. Mangled C++ symbols are
            // the API this crate binds, and weak symbols (COMDAT groups such
            // as `DW.ref.__gxx_personality_v0`, template instantiations) must
            // stay global so the linker can still deduplicate them.
            let listing = String::from_utf8_lossy(&output.stdout);
            let keep_names: Vec<&str> = listing
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split(' ');
                    Some((fields.next()?, fields.next()?))
                })
                .filter(|(name, kind)| {
                    let strong = matches!(*kind, "T" | "D" | "B" | "R");
                    !strong || name.starts_with("_Z") || name.starts_with("lbug_")
                })
                .map(|(name, _)| name)
                .collect();
            if std::fs::write(&keep, keep_names.join("\n")).is_err() {
                return None;
            }
            // `--keep-global-symbols` makes every defined symbol local except
            // the listed ones; undefined references are left alone.
            let status = Command::new("objcopy")
                .arg(format!("--keep-global-symbols={}", keep.display()))
                .arg(&merged)
                .status();
            match status {
                Ok(status) if status.success() => {}
                _ => {
                    println!("cargo:warning=objcopy failed while localizing liblbug symbols; linking it as downloaded");
                    return None;
                }
            }
        }
    }
    Some(out_lib_dir)
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
