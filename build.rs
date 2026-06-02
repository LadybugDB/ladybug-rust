use std::env;
use std::path::{Path, PathBuf};

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
        } else if cfg!(target_os = "macos") {
            println!("cargo:rustc-link-lib=dylib=c++");
        } else {
            println!("cargo:rustc-link-lib=dylib=stdc++");
        }

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

    let script = manifest_dir.join("scripts").join("download_lbug.sh");
    if !script.exists() {
        return false;
    }

    let status = std::process::Command::new("sh")
        .arg(&script)
        .env("LBUG_TARGET_DIR", &lib_dir)
        .current_dir(manifest_dir)
        .status();

    match status {
        Ok(status) if status.success() && lib_path.exists() => true,
        Ok(status) => {
            println!(
                "cargo:warning=Prebuilt liblbug download failed with status {status}; building from source"
            );
            false
        }
        Err(error) => {
            println!(
                "cargo:warning=Could not run prebuilt liblbug downloader ({error}); building from source"
            );
            false
        }
    }
}

fn use_prebuilt_lbug(manifest_dir: &Path) -> Option<Vec<PathBuf>> {
    if !try_download_prebuilt_lbug(manifest_dir) {
        return None;
    }

    let lib_dir = prebuilt_lib_dir(manifest_dir);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
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
        return manifest_dir.join("../..");
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
        build.generator("Ninja");
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
