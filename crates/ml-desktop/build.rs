//! Build script for the `MinLauncher` desktop shell.
//!
//! Tauri needs a build script to embed the application metadata and to
//! validate `tauri.conf.json` against the enabled Cargo features.
//!
//! # Why the icons are staged
//!
//! On Windows the icon is compiled into the executable by the resource
//! compiler — `windres` of the MinGW toolchain, `rc.exe` of the Windows
//! SDK. The compiler writes the path of the icon into a file of its own
//! and reads *that* file through the ANSI code page, so a path with
//! non-ASCII characters is lost on the way: a home directory named
//! `C:\Users\Вася` makes `windres` fail with "can't open icon file:
//! Invalid argument".
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
//!
//! Only the GNU toolchain needs this. The linker of MSVC (`link.exe`) is
//! Unicode-clean, and the import library of the MSVC target of the
//! `windows` crate is found by itself.
//!
//! # Why the `WebView2` loader is copied next to the test binaries
//!
//! `WebView2Loader.dll` is loaded at run time, not at link time. Windows
//! looks for it in the directory of the executable first. The application
//! binary lives in `target/debug`, and Tauri's own build script puts the
//! loader there — but the test binaries live in `target/debug/deps`, and
//! nothing puts the loader there.
//!
//! Without it, every test of this crate dies before the first test runs,
//! with `STATUS_DLL_NOT_FOUND` (0xC0000135). The copy below is what makes
//! `cargo test` work on Windows.
//!
//! # Why the manifest is embedded here
//!
//! Tauri compiles the icon, the version information and the application
//! manifest into one Windows resource, and hands it to the linker with
//! `cargo:rustc-link-arg-bins` — that is, to the *binaries* of the crate
//! only. The test binaries are not among them, so they start without a
//! manifest.
//!
//! That matters because the manifest is what asks Windows for the version 6
//! of the common controls. Without it, the loader falls back to the legacy
//! `comctl32.dll` from `System32`, and the code under test imports
//! `TaskDialogIndirect` — a function that only the version 6 exports. The
//! loader cannot find it and refuses to start the process with
//! `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139), before the first test runs.
//!
//! So Tauri is told to leave the manifest alone, and the manifest below is
//! compiled into a resource of its own and linked into every target of the
//! crate — the application binary and the test binaries alike. The content
//! is the same manifest that Tauri would have used.

use std::path::{Path, PathBuf};

/// The application icon, in the format the resource compiler needs.
const WINDOW_ICON: &str = "icon.ico";

/// The import library that `windows_x86_64_gnu` provides.
const IMPORT_LIBRARY: &str = "libwindows.0.53.0.a";

/// The crate that owns the import library.
const IMPORT_LIBRARY_CRATE: &str = "windows_x86_64_gnu";

/// The loader of `WebView2`, loaded at run time.
const WEBVIEW2_LOADER: &str = "WebView2Loader.dll";

/// The crate that ships the loader.
const WEBVIEW2_CRATE: &str = "webview2-com-sys";

/// The manifest of the application.
///
/// It asks Windows for the version 6 of the common controls, which is the
/// one that exports `TaskDialogIndirect` and the window subclassing
/// functions. Tauri embeds the very same manifest into the application
/// binary; this copy is for the test binaries, which Tauri leaves alone.
const WINDOWS_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>"#;

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

/// Whether the build targets Windows with the GNU toolchain.
///
/// The GNU driver needs the import library of the `windows` crate to be
/// staged under a path it can read; the linker of MSVC finds the library by
/// itself, because it is Unicode-clean.
fn is_windows_gnu() -> bool {
    let target = std::env::var("TARGET").unwrap_or_default();
    target.contains("windows") && target.contains("gnu") && !target.contains("msvc")
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

/// Copy the `WebView2` loader next to the test binaries.
///
/// The loader is loaded at run time, and Windows looks for it in the
/// directory of the executable. The test binaries of the workspace live
/// in `<target>/debug/deps`, and the loader is not there — so every test
/// of this crate would die with `STATUS_ENTRYPOINT_NOT_FOUND` before the
/// first test runs.
///
/// # Errors
/// Fails if the loader cannot be found in the registry.
fn stage_webview2_loader(out_dir: &Path) -> Result<PathBuf, String> {
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
            if name.starts_with(WEBVIEW2_CRATE) {
                candidates.push(entry.path().join("x64").join(WEBVIEW2_LOADER));
            }
        }
    }

    let source = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("{WEBVIEW2_LOADER} was not found in {}", registry.display()))?;

    // `OUT_DIR` is `<target>/debug/build/<crate>-<hash>/out`, so the
    // directory of the test binaries is two levels up.
    let deps = out_dir
        .ancestors()
        .nth(3)
        .map(|root| root.join("deps"))
        .ok_or_else(|| "cannot find the deps directory from OUT_DIR".to_string())?;

    std::fs::create_dir_all(&deps).map_err(|e| format!("cannot create {}: {e}", deps.display()))?;
    let target = deps.join(WEBVIEW2_LOADER);
    std::fs::copy(source, &target).map_err(|e| {
        format!(
            "cannot copy {} to {}: {e}",
            source.display(),
            target.display()
        )
    })?;

    Ok(target)
}

/// Compile the application manifest into a resource of its own and hand
/// it to the linker for every target of the crate.
///
/// Tauri compiles the icon, the version information and the manifest into
/// one resource, and links it into the *binaries* of the crate. The test
/// binaries are left out, and without the manifest they cannot start: the
/// loader falls back to the legacy common controls, which do not export
/// `TaskDialogIndirect`.
///
/// To keep one manifest for the whole crate, Tauri is told not to embed
/// one of its own, and the resource built here is linked into every target
/// instead — the application binary and the test binaries alike.
///
/// The manifest is written to `OUT_DIR` and referenced from a resource file
/// of its own, so that the targets get the manifest and nothing else — the
/// icon and the version information come from Tauri.
///
/// # Errors
/// Fails if the manifest or the resource file cannot be written, or if the
/// resource compiler refuses the resource file.
fn embed_manifest(out_dir: &Path) -> Result<(), String> {
    let manifest = out_dir.join("minlaucher-manifest.xml");
    std::fs::write(&manifest, WINDOWS_MANIFEST)
        .map_err(|e| format!("cannot write {}: {e}", manifest.display()))?;

    // The resource file asks the resource compiler to put the manifest into
    // the resource of type 24 (`RT_MANIFEST`) under the identifier 1, which
    // is where the Windows loader looks for it.
    //
    // The path is written with slashes, because the resource compiler
    // treats a backslash as an escape character and would then look for a
    // file of a different name.
    let resource = out_dir.join("minlaucher-manifest.rc");
    let manifest_path = manifest.to_string_lossy().replace('\\', "/");
    std::fs::write(&resource, format!("1 24 \"{manifest_path}\"\n"))
        .map_err(|e| format!("cannot write {}: {e}", resource.display()))?;

    // `compile_for_everything` links the resource into every target that
    // is linked at all: the application binary, the test binaries, the
    // examples and the dynamic library.
    let result = embed_resource::compile_for_everything(&resource, embed_resource::NONE);
    if let embed_resource::CompilationResult::Failed(error) = result {
        return Err(format!("cannot compile the manifest: {error}"));
    }

    Ok(())
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
    // Tauri links its own manifest into the binaries of the crate only, so
    // the manifest is built below and linked into every target instead.
    let mut windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    match stage_icon(&manifest_dir.join("icons"), &out_dir) {
        Ok(staged) => {
            windows = windows.window_icon_path(staged);
        }
        Err(error) => {
            // Do not fail here: `tauri-build` produces a much better error
            // message for a missing icon, and the icon is only required on
            // Windows.
            println!("cargo:warning={error}");
        }
    }
    attributes = attributes.windows_attributes(windows);

    // Only the GNU toolchain needs the import library to be staged. The
    // linker of MSVC is Unicode-clean and finds the library by itself, so
    // there is nothing to copy there.
    if is_windows_gnu() {
        match stage_import_library(&out_dir) {
            Ok(directory) => {
                // The directory is added to the link search path, so the linker
                // finds the import library under an ASCII-only path even when
                // the registry itself sits below a non-ASCII one.
                println!("cargo:rustc-link-search=native={}", directory.display());
            }
            Err(error) => {
                // Not fatal: when the registry already lives on an ASCII path
                // the linker finds the library by itself, and the staged copy
                // is only needed as a fallback.
                println!("cargo:warning={error}");
            }
        }
    }

    // The loader is needed at run time by the test binaries, which run
    // from `<target>/debug/deps`. Without it they cannot even start.
    #[cfg(target_os = "windows")]
    match stage_webview2_loader(&out_dir) {
        Ok(target) => println!(
            "cargo:warning=webview2 loader staged at {}",
            target.display()
        ),
        Err(error) => println!("cargo:warning={error}"),
    }

    // Tauri links its own manifest into the binaries of the crate only, so
    // the manifest is built here and linked into every target instead.
    #[cfg(target_os = "windows")]
    match embed_manifest(&out_dir) {
        Ok(()) => println!("cargo:warning=manifest embedded into every target"),
        Err(error) => println!("cargo:warning={error}"),
    }

    tauri_build::try_build(attributes).expect("tauri-build failed");
}
