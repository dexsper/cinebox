//! Bundle libmpv for Windows so `libmpv2` + `build_libmpv` can link without a
//! system install. Artifacts land in `$MPV_SOURCE/64` (see `.cargo/config.toml`).

use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

const RELEASE: &str = "20260829";
const GIT: &str = "e8673660ab";
const ARCHIVE_SHA256: &str = "e99b8c85e184463571088c79732f7e1e09ed4524c2945cdca177a4df70ba6f2e";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MPV_SOURCE");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    bundle_windows_libmpv();
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
    println!("cargo:warning=downloading bundled libmpv (~30MB)");
    let agent = match ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(300))
        .build()
        .get(url)
        .set("User-Agent", "cinebox-player-build")
        .call()
    {
        Ok(response) => response,
        Err(error) => panic!("failed to download libmpv from {url}: {error}"),
    };

    let mut file = need(File::create(dest), "create archive file");
    let mut reader = agent.into_reader();
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
    if let Err(error) = io::copy(&mut file, &mut hasher) {
        panic!("hash libmpv archive: {error}");
    }

    format!("{:x}", hasher.finalize())
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
