//! Unpacking the native libraries.
//!
//! Some libraries ship native code in a jar of their own. The launcher
//! unpacks those jars so that the game can load the shared libraries.
//!
//! # Where the files go
//!
//! Modern versions give every native family its own directory, because
//! each library unpacks itself there:
//!
//! ```text
//! natives/
//!   java/    <- java.library.path
//!   jna/     <- jna.tmpdir
//!   lwjgl/   <- org.lwjgl.system.SharedLibraryExtractPath
//!   netty/   <- io.netty.native.workdir
//! ```
//!
//! Older versions point all four properties at the natives directory
//! itself. The launcher reads the arguments of the version it is
//! launching, so the layout follows the version rather than a hard-coded
//! rule.
//!
//! # What is extracted
//!
//! Everything in the jar except the `META-INF` directory, which holds
//! the manifest, the index and the module descriptor. The native files
//! themselves sit at the root of the jar, for example
//! `windows/x64/org/lwjgl/lwjgl.dll`.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use ml_core::{Error, Result};
pub use ml_minecraft::library::NativeFamily;

/// A file that was unpacked from a native jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFile {
    /// The path the file was written to.
    pub path: PathBuf,
    /// The number of bytes written.
    pub size: u64,
    /// The path of the file inside the jar.
    pub entry: String,
}

/// The result of unpacking a whole batch of native jars.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractionReport {
    /// The files that were written.
    pub files: Vec<ExtractedFile>,
    /// The jars that were unpacked.
    pub jars: Vec<PathBuf>,
    /// The number of bytes written.
    pub bytes: u64,
}

impl ExtractionReport {
    /// Whether nothing had to be written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// A native jar that has to be unpacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeJar {
    /// The jar on disk.
    pub path: PathBuf,
    /// The family the jar belongs to.
    pub family: NativeFamily,
}

/// The directories a native family is unpacked into.
///
/// The keys are the system properties the version's arguments use; the
/// values are the directories they resolve to. A version that does not
/// mention a property at all falls back to the natives directory
/// itself, which is what the versions before the split did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeLayout {
    /// The base directory of the natives.
    pub natives_directory: PathBuf,
    /// The property to directory mapping, taken from the arguments.
    pub directories: BTreeMap<String, String>,
}

impl NativeLayout {
    /// The directory a family is unpacked into.
    ///
    /// The property of the family is looked up first; when the version
    /// does not set it, the family falls back to the base directory.
    #[must_use]
    pub fn directory_for(&self, family: NativeFamily) -> PathBuf {
        let property = property_for(family);
        match self.directories.get(property) {
            Some(value) => PathBuf::from(value),
            None => self.natives_directory.join(family.directory()),
        }
    }
}

/// The system property a family is controlled by.
#[must_use]
pub fn property_for(family: NativeFamily) -> &'static str {
    match family {
        NativeFamily::Lwjgl => "org.lwjgl.system.SharedLibraryExtractPath",
        NativeFamily::Jna => "jna.tmpdir",
        NativeFamily::Netty => "io.netty.native.workdir",
        NativeFamily::System => "java.library.path",
    }
}

/// Unpack a batch of native jars.
///
/// A file that already exists with the expected size is left alone, so
/// the work can be repeated cheaply on every launch.
///
/// # Errors
/// Fails if a jar cannot be read or a file cannot be written.
pub fn extract_all(jars: &[NativeJar], layout: &NativeLayout) -> Result<ExtractionReport> {
    let mut report = ExtractionReport::default();

    for jar in jars {
        let directory = layout.directory_for(jar.family);
        let files = extract_jar(&jar.path, &directory)?;
        report.bytes += files.iter().map(|file| file.size).sum::<u64>();
        report.files.extend(files);
        report.jars.push(jar.path.clone());
    }

    Ok(report)
}

/// Unpack one native jar into a directory.
///
/// # Errors
/// Fails if the jar cannot be read or a file cannot be written.
pub fn extract_jar(jar: &Path, directory: &Path) -> Result<Vec<ExtractedFile>> {
    let file = std::fs::File::open(jar).map_err(|e| Error::io(jar, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        Error::InvalidInput(format!("`{}` is not a valid archive: {e}", jar.display()))
    })?;

    let mut written = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| {
            Error::InvalidInput(format!("`{}` has a broken entry: {e}", jar.display()))
        })?;

        let name = entry
            .name()
            .map_err(|e| {
                Error::InvalidInput(format!("`{}` has a broken entry name: {e}", jar.display()))
            })?
            .into_owned();
        if entry.is_dir() || name.starts_with("META-INF/") || name == "META-INF" {
            continue;
        }

        // A jar entry may try to escape the directory; refuse it.
        let relative = Path::new(&name);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(Error::InvalidInput(format!(
                "`{}` contains an unsafe path `{name}`",
                jar.display()
            )));
        }

        let target = directory.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }

        let mut buffer = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buffer)
            .map_err(|e| Error::io(jar, e))?;

        // A file that is already there with the right size is kept.
        if let Ok(metadata) = std::fs::metadata(&target)
            && metadata.len() == buffer.len() as u64
        {
            written.push(ExtractedFile {
                path: target,
                size: buffer.len() as u64,
                entry: name,
            });
            continue;
        }

        std::fs::write(&target, &buffer).map_err(|e| Error::io(&target, e))?;
        written.push(ExtractedFile {
            path: target,
            size: buffer.len() as u64,
            entry: name,
        });
    }

    Ok(written)
}

/// Remove every file of a directory.
///
/// Used to rebuild the natives after a version change, when the layout
/// of the natives may have changed.
///
/// # Errors
/// Fails if a file cannot be removed.
pub fn clean_directory(directory: &Path) -> Result<u64> {
    if !directory.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(directory).map_err(|e| Error::io(directory, e))? {
        let entry = entry.map_err(|e| Error::io(directory, e))?;
        let path = entry.path();
        if path.is_dir() {
            removed += clean_directory(&path)?;
            continue;
        }
        let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
        std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
        removed += size;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_jar(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, '_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn natives_are_unpacked() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("native.jar");
        make_jar(
            &jar,
            &[
                ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n"),
                ("windows/x64/org/lwjgl/lwjgl.dll", b"\x00\x01native"),
            ],
        );

        let target = dir.path().join("natives");
        let files = extract_jar(&jar, &target).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].entry, "windows/x64/org/lwjgl/lwjgl.dll");
        assert!(target.join("windows/x64/org/lwjgl/lwjgl.dll").is_file());
        // The manifest is not part of the natives.
        assert!(!target.join("META-INF/MANIFEST.MF").exists());
    }

    #[test]
    fn an_existing_file_is_not_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("native.jar");
        make_jar(&jar, &[("lwjgl.dll", b"0123456789")]);

        let target = dir.path().join("natives");
        extract_jar(&jar, &target).unwrap();

        // Overwrite the file with a different content of the same size and
        // check that the second run leaves it alone.
        let extracted = target.join("lwjgl.dll");
        std::fs::write(&extracted, b"abcdefghij").unwrap();

        extract_jar(&jar, &target).unwrap();
        assert_eq!(std::fs::read(&extracted).unwrap(), b"abcdefghij");
    }

    #[test]
    fn an_unsafe_path_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("evil.jar");
        make_jar(&jar, &[("../escaped.dll", b"payload")]);

        let error = extract_jar(&jar, &dir.path().join("natives")).unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
        assert!(!dir.path().join("escaped.dll").exists());
    }

    #[test]
    fn the_layout_follows_the_arguments() {
        let layout = NativeLayout {
            natives_directory: PathBuf::from("C:/natives"),
            directories: BTreeMap::from([
                (
                    "java.library.path".to_string(),
                    "C:/natives/java".to_string(),
                ),
                (
                    "org.lwjgl.system.SharedLibraryExtractPath".to_string(),
                    "C:/natives/lwjgl".to_string(),
                ),
            ]),
        };

        assert_eq!(
            layout.directory_for(NativeFamily::System),
            PathBuf::from("C:/natives/java")
        );
        assert_eq!(
            layout.directory_for(NativeFamily::Lwjgl),
            PathBuf::from("C:/natives/lwjgl")
        );
        // A family the version does not mention falls back to the base.
        assert_eq!(
            layout.directory_for(NativeFamily::Jna),
            PathBuf::from("C:/natives/jna")
        );
    }

    #[test]
    fn a_single_directory_layout_puts_everything_together() {
        let layout = NativeLayout {
            natives_directory: PathBuf::from("C:/natives"),
            directories: BTreeMap::from([
                ("java.library.path".to_string(), "C:/natives".to_string()),
                ("jna.tmpdir".to_string(), "C:/natives".to_string()),
                (
                    "org.lwjgl.system.SharedLibraryExtractPath".to_string(),
                    "C:/natives".to_string(),
                ),
                (
                    "io.netty.native.workdir".to_string(),
                    "C:/natives".to_string(),
                ),
            ]),
        };

        for family in [
            NativeFamily::System,
            NativeFamily::Jna,
            NativeFamily::Lwjgl,
            NativeFamily::Netty,
        ] {
            assert_eq!(layout.directory_for(family), PathBuf::from("C:/natives"));
        }
    }

    #[test]
    fn a_batch_is_unpacked_into_the_right_directories() {
        let dir = tempfile::tempdir().unwrap();
        let lwjgl = dir.path().join("lwjgl.jar");
        let jtracy = dir.path().join("jtracy.jar");
        make_jar(&lwjgl, &[("windows/x64/lwjgl.dll", b"lwjgl")]);
        make_jar(&jtracy, &[("jtracy-jni-windows.dll", b"jtracy")]);

        let layout = NativeLayout {
            natives_directory: dir.path().join("natives"),
            directories: BTreeMap::from([
                (
                    "java.library.path".to_string(),
                    dir.path()
                        .join("natives/java")
                        .to_string_lossy()
                        .into_owned(),
                ),
                (
                    "org.lwjgl.system.SharedLibraryExtractPath".to_string(),
                    dir.path()
                        .join("natives/lwjgl")
                        .to_string_lossy()
                        .into_owned(),
                ),
            ]),
        };

        let report = extract_all(
            &[
                NativeJar {
                    path: lwjgl,
                    family: NativeFamily::Lwjgl,
                },
                NativeJar {
                    path: jtracy,
                    family: NativeFamily::System,
                },
            ],
            &layout,
        )
        .unwrap();

        assert_eq!(report.files.len(), 2);
        assert!(
            layout
                .directory_for(NativeFamily::Lwjgl)
                .join("windows/x64/lwjgl.dll")
                .is_file()
        );
        assert!(
            layout
                .directory_for(NativeFamily::System)
                .join("jtracy-jni-windows.dll")
                .is_file()
        );
    }

    #[test]
    fn a_directory_can_be_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("natives");
        std::fs::create_dir_all(target.join("sub")).unwrap();
        std::fs::write(target.join("a.dll"), b"12345").unwrap();
        std::fs::write(target.join("sub/b.dll"), b"123").unwrap();

        let removed = clean_directory(&target).unwrap();
        assert_eq!(removed, 8);
        assert!(!target.join("a.dll").exists());
    }
}
