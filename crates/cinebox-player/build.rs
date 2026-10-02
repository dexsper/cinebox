//! Link libmpv per target OS.
//!
//! Windows: bundle a prebuilt libmpv so `libmpv2` + `build_libmpv` link without a system
//! install. Artifacts land in `$MPV_SOURCE/64` (see `.cargo/config.toml`).
//! Linux: link the system libmpv found through pkg-config.
//! Android: link the libmpv + FFmpeg shared objects from the `dev.jdtech.mpv:libmpv`
//! AAR. Gradle packages the same AAR into the APK (`android/app/build.gradle.kts`),
//! so both pins must change together.

use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

const RELEASE: &str = "20260829";
const GIT: &str = "e8673660ab";
const ARCHIVE_SHA256: &str = "e99b8c85e184463571088c79732f7e1e09ed4524c2945cdca177a4df70ba6f2e";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MPV_SOURCE");

    match need(env::var("CARGO_CFG_TARGET_OS"), "target OS").as_str() {
        "windows" => bundle_windows_libmpv(),
        "linux" => link_system_libmpv(),
        "android" => link_android_libmpv(),
        other => panic!("Cinebox has no libmpv setup for target OS {other}"),
    }
}

/// Client API 2.0 ships with mpv 0.35, the minimum `libmpv2` supports.
const MIN_LIBMPV_API: &str = "2.0";

fn link_system_libmpv() {
    let probe = pkg_config::Config::new()
        .atleast_version(MIN_LIBMPV_API)
        .probe("mpv");

    if let Err(error) = probe {
        panic!(
            "libmpv {MIN_LIBMPV_API}+ (mpv 0.35+) not found via pkg-config: {error}\n\
             Install the development package: libmpv-dev (Debian/Ubuntu), \
             mpv-devel (Fedora), mpv (Arch)."
        );
    }
}

/// Keep in sync with `libmpv` in `android/gradle/libs.versions.toml`.
const ANDROID_AAR_VERSION: &str = "1.0.0";
const ANDROID_AAR_SHA256: &str = "df146592480fc8418415a06b1f1a1d6318b0088e21f52254b0e9a82b61ca8fa2";

/// Only the link-time copies live here; the APK gets its .so files from Gradle.
fn link_android_libmpv() {
    let arch = need(env::var("CARGO_CFG_TARGET_ARCH"), "target arch");
    let abi = match arch.as_str() {
        "aarch64" => "arm64-v8a",
        "arm" => "armeabi-v7a",
        "x86_64" => "x86_64",
        "x86" => "x86",
        other => panic!("libmpv AAR has no build for Android arch {other}"),
    };

    let source = mpv_source_dir();
    let dir = source.join("android").join(abi);
    let libmpv = dir.join("libmpv.so");
    if !libmpv.is_file() {
        let aar = fetch_android_aar(&source);
        extract_android_abi(&aar, abi, &dir);
    }

    if !libmpv.is_file() {
        panic!("libmpv.so missing at {}", libmpv.display());
    }

    println!("cargo:rustc-link-search=native={}", dir.display());
}

fn fetch_android_aar(source: &Path) -> PathBuf {
    let cache = source.join("cache");
    need(fs::create_dir_all(&cache), "create mpv cache");

    let name = format!("libmpv-{ANDROID_AAR_VERSION}.aar");
    let aar = cache.join(&name);
    if aar.is_file() && sha256_file(&aar) != ANDROID_AAR_SHA256 {
        println!("cargo:warning=cached libmpv AAR hash mismatch; re-downloading");
        need(fs::remove_file(&aar), "remove bad AAR");
    }

    if aar.is_file() {
        return aar;
    }

    let url = format!(
        "https://repo1.maven.org/maven2/dev/jdtech/mpv/libmpv/{ANDROID_AAR_VERSION}/{name}"
    );
    download(&url, &aar);

    let hash = sha256_file(&aar);
    if hash != ANDROID_AAR_SHA256 {
        let _ = fs::remove_file(&aar);
        panic!("libmpv AAR sha256 mismatch (got {hash}, expected {ANDROID_AAR_SHA256})");
    }

    aar
}

/// Copy `jni/<abi>/*.so` out of the AAR (a zip archive).
fn extract_android_abi(aar: &Path, abi: &str, dest: &Path) {
    need(fs::create_dir_all(dest), "create android libmpv dir");

    let file = need(File::open(aar), "open libmpv AAR");
    let mut archive = need(zip::ZipArchive::new(file), "read libmpv AAR");
    let prefix = format!("jni/{abi}/");

    for index in 0..archive.len() {
        let mut entry = need(archive.by_index(index), "read AAR entry");
        let Some(name) = entry.name().strip_prefix(&prefix).map(str::to_owned) else {
            continue;
        };
        if name.is_empty() || name.contains('/') {
            continue;
        }

        let mut out = need(File::create(dest.join(&name)), "create extracted .so");
        need(io::copy(&mut entry, &mut out), "extract .so from AAR");
    }
}

fn bundle_windows_libmpv() {
    let pointer_width = need(env::var("CARGO_CFG_TARGET_POINTER_WIDTH"), "pointer width");
    if pointer_width != "64" {
        panic!("Cinebox bundles 64-bit libmpv only (got pointer width {pointer_width})");
    }

    let source = mpv_source_dir();
    let dir64 = source.join("64");
    need(fs::create_dir_all(&dir64), "create MPV_SOURCE/64");

    let dll = dir64.join("libmpv-2.dll");
    let implib = dir64.join("mpv.lib");
    if !(dll.is_file() && implib.is_file()) {
        fetch_and_prepare(&source, &dir64);
    }

    if !dll.is_file() {
        panic!("bundled libmpv-2.dll missing at {}", dll.display());
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") && !implib.is_file() {
        panic!("bundled mpv.lib missing at {}", implib.display());
    }

    println!("cargo:rustc-link-search=native={}", dir64.display());
    copy_runtime_dlls(&dir64);
}

fn mpv_source_dir() -> PathBuf {
    let raw = match env::var("MPV_SOURCE") {
        Ok(source) => PathBuf::from(source),
        Err(_) => PathBuf::from(need(env::var("CARGO_MANIFEST_DIR"), "CARGO_MANIFEST_DIR"))
            .join("mpv-src"),
    };
    need(fs::create_dir_all(&raw), "create MPV_SOURCE");
    need(fs::canonicalize(&raw), "canonicalize MPV_SOURCE")
}

fn fetch_and_prepare(source: &Path, dir64: &Path) {
    let cache = source.join("cache");
    need(fs::create_dir_all(&cache), "create mpv cache");

    let name = format!("mpv-dev-x86_64-{RELEASE}-git-{GIT}.7z");
    let archive = cache.join(&name);
    if archive.is_file() {
        let hash = sha256_file(&archive);
        if hash != ARCHIVE_SHA256 {
            println!("cargo:warning=cached libmpv archive hash mismatch; re-downloading");
            need(fs::remove_file(&archive), "remove bad archive");
        }
    }

    if !archive.is_file() {
        let url = format!(
            "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/{RELEASE}/{name}"
        );

        download(&url, &archive);
        let hash = sha256_file(&archive);

        if hash != ARCHIVE_SHA256 {
            let _ = fs::remove_file(&archive);
            panic!("libmpv archive sha256 mismatch (got {hash}, expected {ARCHIVE_SHA256})");
        }
    }

    let extract = source.join("extract");
    if extract.exists() {
        need(fs::remove_dir_all(&extract), "clear extract dir");
    }

    need(fs::create_dir_all(&extract), "create extract dir");
    println!("cargo:warning=extracting bundled libmpv");
    if let Err(error) = sevenz_rust2::decompress_file(&archive, &extract) {
        panic!("failed to extract libmpv archive: {error}");
    }

    let found_dll = find_named(&extract, "libmpv-2.dll");
    let Some(found_dll) = found_dll else {
        panic!("libmpv-2.dll not found in {}", extract.display());
    };

    copy_file(&found_dll, &dir64.join("libmpv-2.dll"));
    for dll in find_dlls(&extract) {
        let Some(name) = dll.file_name() else {
            continue;
        };
        copy_file(&dll, &dir64.join(name));
    }

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "msvc" {
        generate_msvc_implib(&dir64.join("libmpv-2.dll"), dir64);
    } else if let Some(mingw) = find_named(&extract, "libmpv.dll.a") {
        copy_file(&mingw, &dir64.join("libmpv.dll.a"));
    }

    let _ = fs::remove_dir_all(&extract);
}

fn generate_msvc_implib(dll: &Path, dir64: &Path) {
    let bytes = need(fs::read(dll), "read libmpv-2.dll");
    let pe = match goblin::pe::PE::parse(&bytes) {
        Ok(pe) => pe,
        Err(error) => panic!("parse libmpv-2.dll: {error}"),
    };

    let def_path = dir64.join("mpv.def");
    let mut def = String::from("LIBRARY libmpv-2.dll\nEXPORTS\n");
    let mut count = 0u32;
    for export in &pe.exports {
        let Some(name) = export.name else {
            continue;
        };
        if !name.starts_with("mpv_") {
            continue;
        }
        def.push_str("    ");
        def.push_str(name);
        def.push('\n');
        count += 1;
    }

    if count == 0 {
        panic!("no mpv_* exports in {}", dll.display());
    }

    need(fs::write(&def_path, def), "write mpv.def");
    let target = need(env::var("TARGET"), "TARGET");
    let Some(lib) = cc::windows_registry::find_tool(&target, "lib.exe") else {
        panic!("MSVC lib.exe not found; install Visual Studio C++ build tools");
    };

    let mut cmd = lib.to_command();
    cmd.current_dir(dir64);
    cmd.args(["/NOLOGO", "/DEF:mpv.def", "/OUT:mpv.lib", "/MACHINE:X64"]);

    let status = need(cmd.status(), "run lib.exe");
    if !status.success() {
        panic!("lib.exe failed with {status}");
    }
}

fn copy_runtime_dlls(dir64: &Path) {
    let profile = profile_dir();
    let deps = profile.join("deps");
    need(fs::create_dir_all(&deps), "create target deps dir");

    for dll in find_dlls(dir64) {
        let Some(name) = dll.file_name() else {
            continue;
        };
        copy_file(&dll, &profile.join(name));
        copy_file(&dll, &deps.join(name));
    }
}

fn profile_dir() -> PathBuf {
    let out = PathBuf::from(need(env::var("OUT_DIR"), "OUT_DIR"));
    match out.ancestors().nth(3) {
        Some(path) => path.to_path_buf(),
        None => panic!(
            "OUT_DIR is not in the expected cargo layout: {}",
            out.display()
        ),
    }
}

fn download(url: &str, dest: &Path) {
    println!("cargo:warning=downloading bundled libmpv (30-50MB)");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .new_agent();

    let mut response = match agent
        .get(url)
        .header("User-Agent", "cinebox-player-build")
        .call()
    {
        Ok(response) => response,
        Err(error) => panic!("failed to download libmpv from {url}: {error}"),
    };

    let mut file = need(File::create(dest), "create archive file");
    let mut reader = response.body_mut().as_reader();
    if let Err(error) = io::copy(&mut reader, &mut file) {
        let _ = fs::remove_file(dest);
        panic!("failed to save libmpv archive: {error}");
    }

    if let Err(error) = file.flush() {
        panic!("failed to flush libmpv archive: {error}");
    }
}

fn sha256_file(path: &Path) -> String {
    let mut file = need(File::open(path), "open archive for hash");
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) => panic!("hash libmpv archive: {error}"),
        };
        hasher.update(&buf[..n]);
    }

    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            hex.push_str(&format!("{byte:02x}"));
            hex
        })
}

fn find_named(root: &Path, file_name: &str) -> Option<PathBuf> {
    let mut found = None;
    walk(root, &mut |path| {
        if path.file_name().is_some_and(|name| name == file_name) {
            found = Some(path.to_path_buf());
        }
    });

    found
}

fn find_dlls(root: &Path) -> Vec<PathBuf> {
    let mut dlls = Vec::new();
    walk(root, &mut |path| {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        if name.ends_with(".dll") || name.ends_with(".DLL") {
            dlls.push(path.to_path_buf());
        }
    });

    dlls
}

fn walk(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
            continue;
        }
        visit(&path);
    }
}

fn copy_file(src: &Path, dest: &Path) {
    if src == dest {
        return;
    }

    if let Some(parent) = dest.parent() {
        need(fs::create_dir_all(parent), "create copy dest dir");
    }

    if let Err(error) = fs::copy(src, dest) {
        panic!("copy {} -> {}: {error}", src.display(), dest.display());
    }
}

fn need<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{what}: {error}"),
    }
}
