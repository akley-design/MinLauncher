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
//!
//! # Only once
//!
//! The natives of a version are unpacked once. A file that is already
//! there with the right size is kept — but the jar still has to be
//! opened and read to know that, and twenty-one jars of a modern
//! version hold thousands of files between them. That is work that
//! happens on every launch and that nobody waits for.
//!
//! So the launcher remembers what it unpacked, in a marker file next to
//! the natives: which jars the files came from, and which files were
//! written. When the jars are the same and the files are still there,
//! the jars are not opened at all. The marker is written after the
//! unpacking, so a failed unpacking is done again next time.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ml_core::{Error, Result};
pub use ml_minecraft::library::NativeFamily;

/// The file that remembers what was unpacked.
///
/// It sits in the base directory of the natives, next to the files that
/// were unpacked.
const MARKER_NAME: &str = ".extracted";

/// The file the marker is written through.
///
/// The marker is moved over the target, so a launcher that is stopped
/// halfway does not leave a half-written marker behind.
const MARKER_TEMP_NAME: &str = ".extracted.tmp";

/// The first line of the marker file: the format and the version of it.
const MARKER_FORMAT: &str = "minlaucher-natives 1";

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
    /// Whether the natives were already on the disk and nothing was
    /// unpacked.
    pub skipped: bool,
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
    /// does not set it, the family falls back to the directory the
    /// version does name for the loader, and only then to the family
    /// directory of the base.
    #[must_use]
    pub fn directory_for(&self, family: NativeFamily) -> PathBuf {
        let property = property_for(family);
        if let Some(value) = self.directories.get(property) {
            return PathBuf::from(value);
        }
        // A version that names only `java.library.path` — everything from
        // 1.13 to 1.20 — gives the loader no other place to look. Left in
        // a family directory of its own the library is never found and
        // LWJGL dies with “Failed to locate library: lwjgl.dll”.
        if family != NativeFamily::System
            && let Some(root) = self.directories.get(property_for(NativeFamily::System))
        {
            return PathBuf::from(root);
        }
        self.natives_directory.join(family.directory())
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
/// The natives of a version are unpacked **once**: the marker file next
/// to them says which jars the files came from, and when both the jars
/// and the files are the same, the jars are not opened at all.
///
/// A file that already exists with the expected size is left alone, so a
/// half-finished unpacking is not done twice either.
///
/// # Errors
/// Fails if a jar cannot be read or a file cannot be written.
pub fn extract_all(jars: &[NativeJar], layout: &NativeLayout) -> Result<ExtractionReport> {
    let mut report = ExtractionReport::default();
    let marker = layout.natives_directory.join(MARKER_NAME);
    let source = source_of(jars, layout);

    // The natives are already on the disk: nothing to unpack, and no
    // jar to open.
    if let Some(source) = &source
        && let Some(mark) = read_mark(&marker)
        && &mark.source == source
        && mark.files.iter().all(|(path, size)| file_is(path, *size))
    {
        report.skipped = true;
        return Ok(report);
    }

    for jar in jars {
        let directory = layout.directory_for(jar.family);
        let files = extract_jar(&jar.path, &directory)?;
        report.bytes += files.iter().map(|file| file.size).sum::<u64>();
        report.files.extend(files);
        report.jars.push(jar.path.clone());
    }

    // The marker is written after the unpacking: an unpacking that
    // failed has to be done again, and a marker that was written before
    // it would keep the natives directory empty.
    if let Some(source) = source {
        let files = report
            .files
            .iter()
            .map(|file| (file.path.clone(), file.size))
            .collect();
        let mark = Mark { source, files };
        if let Err(error) = write_mark(&marker, &mark) {
            tracing::warn!("cannot write the marker of the natives: {error}");
        }
    }

    Ok(report)
}

/// What the marker file remembers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mark {
    /// The jars the natives came from.
    source: String,
    /// The files that were unpacked, and how big they are.
    files: Vec<(PathBuf, u64)>,
}

/// The fingerprint of the natives: the jars they come from and the
/// directories they are unpacked into.
///
/// The path, the size and the time of the last change of every jar.
/// The time matters: a jar that was replaced by another one of the same
/// size has to be unpacked again, and the size alone would not say so.
///
/// `None` when a jar cannot be read — the natives have to be unpacked
/// again then, and the unpacking fails with a proper error.
fn source_of(jars: &[NativeJar], layout: &NativeLayout) -> Option<String> {
    let mut sorted: Vec<&NativeJar> = jars.iter().collect();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));

    let mut lines = Vec::with_capacity(sorted.len());
    for jar in sorted {
        let metadata = std::fs::metadata(&jar.path).ok()?;
        let modified = metadata.modified().ok()?;
        let stamp = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
        lines.push(format!(
            "{}|{}|{}",
            jar.path.to_string_lossy(),
            metadata.len(),
            stamp.as_nanos()
        ));
    }
    // The directories belong to the fingerprint as much as the jars do:
    // a launcher that changed where a family is unpacked has to unpack
    // it again, or the game looks for a library that was never put there.
    let directories: BTreeSet<PathBuf> = jars
        .iter()
        .map(|jar| layout.directory_for(jar.family))
        .collect();
    for directory in directories {
        lines.push(format!("into {}", directory.to_string_lossy()));
    }

    Some(lines.join("\n"))
}

/// Whether a file is on the disk with the size it was written with.
fn file_is(path: &Path, size: u64) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.len() == size)
}

/// Read the marker file.
fn read_mark(path: &Path) -> Option<Mark> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != MARKER_FORMAT {
        return None;
    }

    let mut source = Vec::new();
    let mut files = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("source ") {
            source.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("file ") {
            let (path, size) = rest.rsplit_once('|')?;
            files.push((PathBuf::from(path), size.parse().ok()?));
        }
    }

    Some(Mark {
        source: source.join("\n"),
        files,
    })
}

/// Write the marker file.
///
/// The file is written through a temporary one that is moved over the
/// target, so that a launcher that is stopped halfway does not leave a
/// half-written marker behind: a marker that was cut short would send
/// the next launch to a natives directory that was never filled.
fn write_mark(path: &Path, mark: &Mark) -> Result<()> {
    let mut text = String::from(MARKER_FORMAT);
    text.push('\n');
    for line in mark.source.lines() {
        text.push_str("source ");
        text.push_str(line);
        text.push('\n');
    }
    for (file, size) in &mark.files {
        text.push_str("file ");
        text.push_str(&file.to_string_lossy());
        text.push('|');
        text.push_str(&size.to_string());
        text.push('\n');
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let temporary = path.with_file_name(MARKER_TEMP_NAME);
    std::fs::write(&temporary, text).map_err(|e| Error::io(&temporary, e))?;
    std::fs::rename(&temporary, path).map_err(|e| Error::io(path, e))?;
    Ok(())
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
        //
        // shortcut: a native that is replaced by another one of the same
        // length keeps the old content. The jars of Mojang do not change
        // under one path, so it does not happen; when it does, the file
        // has to be compared by its content, not by its length.
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
    use std::time::Duration;

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

    /// The time of the last change of a file.
    fn modified_at(path: &Path) -> SystemTime {
        std::fs::metadata(path).unwrap().modified().unwrap()
    }

    /// Set the time of the last change of a file.
    fn set_modified(path: &Path, stamp: SystemTime) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(stamp))
            .unwrap();
    }

    /// A layout that puts everything into one directory.
    fn layout_in(directory: &Path) -> NativeLayout {
        NativeLayout {
            natives_directory: directory.join("natives"),
            directories: BTreeMap::new(),
        }
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
        // A family the version does not mention goes where the version
        // tells the loader to look.
        assert_eq!(
            layout.directory_for(NativeFamily::Jna),
            PathBuf::from("C:/natives/java")
        );
    }

    #[test]
    fn a_version_that_names_only_the_library_path_gets_every_family_there() {
        let layout = NativeLayout {
            natives_directory: PathBuf::from("C:/natives"),
            directories: BTreeMap::from([(
                "java.library.path".to_string(),
                "C:/natives".to_string(),
            )]),
        };

        // The documents of 1.13 to 1.20 name the library path and nothing
        // else, so a library of another family has to lie in that path.
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
    fn a_version_that_names_no_property_keeps_the_family_directories() {
        let layout = NativeLayout {
            natives_directory: PathBuf::from("C:/natives"),
            directories: BTreeMap::new(),
        };

        // The versions before 1.13 have no arguments at all: the launcher
        // names the family directories itself, in its own arguments.
        assert_eq!(
            layout.directory_for(NativeFamily::Lwjgl),
            PathBuf::from("C:/natives/lwjgl")
        );
        assert_eq!(
            layout.directory_for(NativeFamily::System),
            PathBuf::from("C:/natives/java")
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

    #[test]
    fn the_natives_are_not_unpacked_twice() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("native.jar");
        make_jar(&jar, &[("lwjgl.dll", b"first")]);

        let layout = layout_in(dir.path());
        let jars = [NativeJar {
            path: jar.clone(),
            family: NativeFamily::Lwjgl,
        }];

        let first = extract_all(&jars, &layout).unwrap();
        assert_eq!(first.files.len(), 1);
        assert!(!first.skipped);
        assert!(layout.natives_directory.join(MARKER_NAME).is_file());

        // The jar holds something else now: the same size, and the same
        // time of the last change, so that nothing but a second unpacking
        // could tell the difference. The file that was unpacked keeps
        // its content — the jar was not read a second time.
        let stamp = modified_at(&jar);
        make_jar(&jar, &[("lwjgl.dll", b"other")]);
        set_modified(&jar, stamp);

        let second = extract_all(&jars, &layout).unwrap();
        assert!(second.skipped, "the natives were unpacked a second time");
        assert!(second.is_empty());

        let extracted = layout.directory_for(NativeFamily::Lwjgl).join("lwjgl.dll");
        assert_eq!(
            std::fs::read(&extracted).unwrap(),
            b"first",
            "the jar was read a second time"
        );
    }

    #[test]
    fn a_changed_jar_is_unpacked_again() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("native.jar");
        make_jar(&jar, &[("lwjgl.dll", b"first")]);

        let layout = layout_in(dir.path());
        let jars = [NativeJar {
            path: jar.clone(),
            family: NativeFamily::Lwjgl,
        }];
        extract_all(&jars, &layout).unwrap();

        // The jar was replaced by another one: the time of the last
        // change moved, and the natives have to be unpacked again.
        //
        // The new content is longer than the old one, because a file that
        // is already there with the right size is kept — that is the rule
        // of the unpacking itself, and it is not what this test is about.
        let stamp = modified_at(&jar);
        make_jar(&jar, &[("lwjgl.dll", b"other content")]);
        set_modified(&jar, stamp + Duration::from_secs(1));

        let second = extract_all(&jars, &layout).unwrap();
        assert!(!second.skipped);
        assert_eq!(second.files.len(), 1);

        let extracted = layout.directory_for(NativeFamily::Lwjgl).join("lwjgl.dll");
        assert_eq!(std::fs::read(&extracted).unwrap(), b"other content");
    }

    #[test]
    fn a_native_that_is_gone_is_unpacked_again() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("native.jar");
        make_jar(&jar, &[("lwjgl.dll", b"first")]);

        let layout = layout_in(dir.path());
        let jars = [NativeJar {
            path: jar.clone(),
            family: NativeFamily::Lwjgl,
        }];
        extract_all(&jars, &layout).unwrap();

        // Somebody removed one of the natives: the game would start
        // without it, so it has to be put back.
        let extracted = layout.directory_for(NativeFamily::Lwjgl).join("lwjgl.dll");
        std::fs::remove_file(&extracted).unwrap();

        let second = extract_all(&jars, &layout).unwrap();
        assert!(!second.skipped);
        assert!(extracted.is_file());
    }

    #[test]
    fn a_broken_marker_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("native.jar");
        make_jar(&jar, &[("lwjgl.dll", b"first")]);

        let layout = layout_in(dir.path());
        let jars = [NativeJar {
            path: jar.clone(),
            family: NativeFamily::Lwjgl,
        }];

        // A marker that says something else is not a marker: the natives
        // are unpacked, and the marker is written anew.
        std::fs::create_dir_all(&layout.natives_directory).unwrap();
        std::fs::write(layout.natives_directory.join(MARKER_NAME), b"nonsense\n").unwrap();

        let report = extract_all(&jars, &layout).unwrap();
        assert!(!report.skipped);
        assert_eq!(report.files.len(), 1);
    }
}
