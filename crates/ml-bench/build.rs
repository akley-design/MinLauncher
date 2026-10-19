//! Build script for the benchmark harness.
//!
//! # Why the import libraries are staged
//!
//! `criterion` measures the resident memory of the process, and does it
//! through `page_size`, which uses the `winapi` crate on Windows. The
//! `winapi` crate ships its import libraries inside the Cargo registry,
//! and the registry lives under `$CARGO_HOME` — on this machine
//! `C:\Users\Вася\.cargo`, a path with non-ASCII characters.
//!
//! The MinGW driver decodes its command line using the Windows ANSI code
//! page while the Rust compiler encodes the path as UTF-8. The two
//! encodings disagree about the non-ASCII part, so the driver looks for a
//! directory that does not exist and the link fails with
//! "cannot find `-lwinapi_kernel32`: No such file or directory".
//!
//! The fix is the same as in `ml-desktop`: copy the import libraries into
//! `OUT_DIR`, which the build script controls and which lives on an
//! ASCII-only path, and add that directory to the link search path. The
//! linker then resolves the libraries through a path it can read.
//!
//! The libraries are only needed on Windows, and only when the registry
//! itself sits below a non-ASCII path; on a machine with an ASCII
//! `CARGO_HOME` the staging is a harmless copy.

use std::path::{Path, PathBuf};

/// The crate that owns the import libraries.
const IMPORT_LIBRARY_CRATE: &str = "winapi-x86_64-pc-windows-gnu";

/// The import libraries the benchmark needs.
///
/// The `winapi` crate maps its modules onto the Windows API sets, and
/// the linker asks for them by the name of the module that uses them.
/// `page_size` uses the `sysinfoapi` module, which the linker requests
/// as `winapi_kernel32`.
const IMPORT_LIBRARIES: &[&str] = &["libwinapi_kernel32.a"];

/// Copy the Windows import libraries into `OUT_DIR` and return the
/// directory that now holds them.
///
/// The libraries are looked up inside the Cargo registry, where `winapi`
/// unpacks them. The registry directory name contains a hash that
/// changes between Cargo versions, so every candidate is tried.
///
/// # Errors
/// Fails if the libraries cannot be found or cannot be copied.
fn stage_import_libraries(out_dir: &Path) -> Result<PathBuf, String> {
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
                candidates.push(entry.path().join("lib"));
            }
        }
    }

    let target_dir = out_dir.join("import-libs");
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("cannot create {}: {e}", target_dir.display()))?;

    for library in IMPORT_LIBRARIES {
        let source = candidates
            .iter()
            .map(|directory| directory.join(library))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| format!("{library} was not found in {}", registry.display()))?;

        let target = target_dir.join(library);
        std::fs::copy(&source, &target).map_err(|e| {
            format!(
                "cannot copy {} to {}: {e}",
                source.display(),
                target.display()
            )
        })?;
    }

    Ok(target_dir)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");

    // The import libraries only exist on Windows.
    if !cfg!(windows) {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set by cargo"));

    match stage_import_libraries(&out_dir) {
        Ok(directory) => {
            // The directory is added to the link search path, so the linker
            // finds the import libraries under an ASCII-only path even when
            // the registry itself sits below a non-ASCII one.
            println!("cargo:rustc-link-search=native={}", directory.display());
        }
        Err(error) => {
            // Not fatal: when the registry already lives on an ASCII path
            // the linker finds the libraries by itself, and the staged copy
            // is only needed as a fallback.
            println!("cargo:warning={error}");
        }
    }
}
