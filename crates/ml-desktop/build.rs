//! Build script for the MinLauncher desktop shell.
//!
//! Tauri needs a build script to embed the application metadata and to
//! validate `tauri.conf.json` against the enabled Cargo features.
//!
//! # Why the icons are staged
//!
//! On Windows the icon is compiled into the executable by `windres`, which is
//! part of the MinGW toolchain. Most MinGW builds are linked against the
//! ANSI code page and therefore cannot open a file whose path contains
//! non-ASCII characters — a home directory named `C:\Users\Вася` makes
//! `windres` fail with "can't open icon file: Invalid argument".
//!
//! The fix is to copy the icon into `OUT_DIR`, which lives under the Cargo
//! target directory, and to hand the staged copy to `tauri-build` through
//! [`tauri_build::WindowsAttributes::window_icon_path`]. That attribute is
//! used verbatim, whereas the path taken from `bundle.icon` in
//! `tauri.conf.json` is resolved against the crate directory and would
//! therefore still contain the non-ASCII prefix.
//!
//! The target directory is a build artifact and can be pointed at an ASCII
//! path independently of where the sources live, so the staging works no
//! matter where the repository is checked out.
//!
//! # Why the import libraries are staged
//!
//! The `windows` crate pulls in `windows_x86_64_gnu`, which ships the
//! import library `libwindows.0.53.0.a` inside the Cargo registry. The
//! registry lives under `$CARGO_HOME`, and on this machine `$CARGO_HOME` is
//! `C:\Users\Вася\.cargo` — a path with non-ASCII characters.
//!
//! That matters because the MinGW driver decodes its command line using the
//! Windows ANSI code page (1251 here), while the Rust compiler encodes the
//! path as UTF-8. The two encodings disagree about the non-ASCII part, so
//! the driver ends up looking for a directory that does not exist and the
//! link fails with
//! "cannot find -lwindows.0.53.0: No such file or directory".
//!
//! The fix is the same as for the icon: copy the import library into
//! `OUT_DIR`, which the build script controls, and add that directory to
//! the link search path. The linker then resolves the library through an
//! ASCII-only path.

use std::path::{Path, PathBuf};

/// The application icon, in the format `windres` needs.
const WINDOW_ICON: &str = "icon.ico";

/// The import library that `windows_x86_64_gnu` provides.
const IMPORT_LIBRARY: &str = "libwindows.0.53.0.a";

/// The crate that owns the import library.
const IMPORT_LIBRARY_CRATE: &str = "windows_x86_64_gnu";

/// Copy the application icon into `OUT_DIR` and return the new location.
///
/// # Errors
/// Fails if the icon is missing or cannot be copied.
fn stage_icon(source_dir: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let source = source_dir.join(WINDOW_ICON);
    if !source.is_file() {
        return Err(format!("icon {} is missing", source.display()));
    }
    let target_dir = out_dir.join("icons");
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("cannot create {}: {e}", target_dir.display()))?;
    let target = target_dir.join(WINDOW_ICON);
    std::fs::copy(&source, &target).map_err(|e| {
        format!(
            "cannot copy {} to {}: {e}",
            source.display(),
            target.display()
        )
    })?;
    Ok(target)
}

/// Copy the Windows import library into `OUT_DIR` and return the directory
/// that now holds it.
///
/// The library is looked up inside the Cargo registry, which is where
/// `windows_x86_64_gnu` unpacks it. The registry directory name contains a
/// hash that changes between Cargo versions, so every candidate is tried.
///
/// # Errors
/// Fails if the library cannot be found or cannot be copied.
fn stage_import_library(out_dir: &Path) -> Result<PathBuf, String> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .ok_or_else(|| "neither CARGO_HOME nor HOME is set".to_string())?;

    let registry = cargo_home.join("registry").join("src");
    let sources = std::fs::read_dir(&registry)
        .map_err(|e| format!("cannot read {}: {e}", registry.display()))?;

    let mut candidates: Vec<PathBuf> = Vec::new();
    for source in sources {
        let source = source.map_err(|e| format!("cannot read {}: {e}", registry.display()))?;
        let source = source.path();
        let Ok(entries) = std::fs::read_dir(&source) else {
            continue;
        };
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot read {}: {e}", source.display()))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(IMPORT_LIBRARY_CRATE) {
                candidates.push(entry.path().join("lib").join(IMPORT_LIBRARY));
            }
        }
    }

    let source = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("{IMPORT_LIBRARY} was not found in {}", registry.display()))?;

    let target_dir = out_dir.join("import-libs");
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("cannot create {}: {e}", target_dir.display()))?;
    let target = target_dir.join(IMPORT_LIBRARY);
    std::fs::copy(source, &target).map_err(|e| {
        format!(
            "cannot copy {} to {}: {e}",
            source.display(),
            target.display()
        )
    })?;

    Ok(target_dir)
}

/// Generate the Tauri build-time artifacts.
fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo"),
    );
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set by cargo"));

    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=icons/{WINDOW_ICON}");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");

    let mut attributes = tauri_build::Attributes::new();
    match stage_icon(&manifest_dir.join("icons"), &out_dir) {
        Ok(staged) => {
            attributes = attributes
                .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(staged));
        }
        Err(error) => {
            // Do not fail here: `tauri-build` produces a much better error
            // message for a missing icon, and the icon is only required on
            // Windows.
            println!("cargo:warning={error}");
        }
    }

    match stage_import_library(&out_dir) {
        Ok(directory) => {
            // The directory is added to the link search path, so the linker
            // finds the import library under an ASCII-only path even when
            // the registry itself sits below a non-ASCII one.
            println!("cargo:rustc-link-search=native={}", directory.display());
        }
        Err(error) => {
            // Not fatal: when the registry already lives on an ASCII path the
            // linker finds the library by itself, and the staged copy is
            // only needed as a fallback.
            println!("cargo:warning={error}");
        }
    }

    tauri_build::try_build(attributes).expect("tauri-build failed");
}
