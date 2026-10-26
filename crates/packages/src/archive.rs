//! Path-safe zip extraction and creation (`.fergplugin` / `.fergskill`).

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::PackageError;

/// Hard limits applied to any archive before it is unpacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    /// Maximum number of files.
    pub max_files: usize,
    /// Maximum total unpacked size in bytes.
    pub max_total_bytes: u64,
    /// Maximum size of a single file in bytes.
    pub max_file_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_files: 2_000,
            max_total_bytes: 64 * 1024 * 1024,
            max_file_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Extract `bytes` into `dest`, rejecting unsafe paths and oversized entries.
///
/// Returns the number of files written. Existing files are never overwritten:
/// extraction is only used for fresh install directories.
pub fn extract_archive(
    bytes: &[u8],
    dest: &Path,
    limits: &ArchiveLimits,
) -> Result<usize, PackageError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|err| PackageError::InvalidArchive(err.to_string()))?;
    if archive.len() > limits.max_files {
        return Err(PackageError::TooLarge(format!(
            "{} entries exceeds {}",
            archive.len(),
            limits.max_files
        )));
    }
    fs::create_dir_all(dest)?;
    let root = dest.canonicalize()?;
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut total: u64 = 0;
    let mut written = 0usize;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| PackageError::InvalidArchive(err.to_string()))?;
        let name = entry.name().to_string();
        let relative = normalize_entry(&name)?;

        if is_symlink(&entry) {
            return Err(PackageError::UnsupportedEntry(relative));
        }

        let target = dest.join(&relative);
        if !target.starts_with(dest) && !target.starts_with(&root) {
            return Err(PackageError::UnsafePath(name));
        }

        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }

        if !seen.insert(relative.clone()) {
            return Err(PackageError::DuplicateEntry(relative.display().to_string()));
        }
        let declared = entry.size();
        if declared > limits.max_file_bytes {
            return Err(PackageError::TooLarge(format!(
                "{} is {} bytes (max {})",
                relative.display(),
                declared,
                limits.max_file_bytes
            )));
        }
        total = total.saturating_add(declared);
        if total > limits.max_total_bytes {
            return Err(PackageError::TooLarge(format!(
                "unpacked size exceeds {} bytes",
                limits.max_total_bytes
            )));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::options().write(true).create_new(true).open(&target)?;
        let mut buffer = [0u8; 8 * 1024];
        let mut written_bytes: u64 = 0;
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            written_bytes = written_bytes.saturating_add(read as u64);
            if written_bytes > limits.max_file_bytes {
                return Err(PackageError::TooLarge(format!(
                    "{} exceeds {} bytes",
                    relative.display(),
                    limits.max_file_bytes
                )));
            }
            output.write_all(&buffer[..read])?;
        }
        written += 1;
    }

    Ok(written)
}

/// Create a zip archive from `source` at `out`.
///
/// When `include_signature` is false, `signature.ed25519` is omitted so the
/// archive can be signed afterwards.
pub fn create_archive(
    source: &Path,
    out: &Path,
    include_signature: bool,
) -> Result<(), PackageError> {
    let file = File::create(out)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut entries: Vec<PathBuf> = walkdir::WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .collect();
    entries.sort();

    for path in entries {
        let relative = path
            .strip_prefix(source)
            .map_err(|_| PackageError::UnsafePath(path.display().to_string()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if !include_signature && name == crate::digest::SIGNATURE_FILE {
            continue;
        }
        let mut input = File::open(&path)?;
        writer
            .start_file(name, options)
            .map_err(|err| PackageError::InvalidArchive(err.to_string()))?;
        std::io::copy(&mut input, &mut writer)?;
    }

    writer
        .finish()
        .map_err(|err| PackageError::InvalidArchive(err.to_string()))?;
    Ok(())
}

/// Normalize an archive entry name into a relative path.
fn normalize_entry(name: &str) -> Result<PathBuf, PackageError> {
    if name.is_empty() || name.contains('\0') {
        return Err(PackageError::UnsafePath(name.to_string()));
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(PackageError::UnsafePath(name.to_string()));
    }
    let normalized = name.replace('\\', "/");
    let mut out = PathBuf::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(segment) => out.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PackageError::UnsafePath(name.to_string()))
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(PackageError::UnsafePath(name.to_string()));
    }
    Ok(out)
}

fn is_symlink<R: std::io::Read>(entry: &zip::read::ZipFile<'_, R>) -> bool {
    entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip_with(name: &str, body: &[u8]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buffer);
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body).unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn extracts_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let count = extract_archive(
            &zip_with("plugin.toml", b"x"),
            dir.path(),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(fs::read(dir.path().join("plugin.toml")).unwrap(), b"x");
    }

    #[test]
    fn rejects_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let err = extract_archive(
            &zip_with("../escape.txt", b"x"),
            dir.path(),
            &Default::default(),
        )
        .unwrap_err();
        assert!(matches!(err, PackageError::UnsafePath(_)));
        assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
    }

    #[test]
    fn rejects_absolute_paths() {
        let err = extract_archive(
            &zip_with("/etc/passwd", b"x"),
            tempfile::tempdir().unwrap().path(),
            &Default::default(),
        )
        .unwrap_err();
        assert!(matches!(err, PackageError::UnsafePath(_)));
    }

    #[test]
    fn rejects_duplicate_entries() {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buffer);
            for name in ["a.txt", "./a.txt"] {
                writer
                    .start_file(name, SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(b"x").unwrap();
            }
            writer.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let err =
            extract_archive(&buffer.into_inner(), dir.path(), &Default::default()).unwrap_err();
        assert!(matches!(err, PackageError::DuplicateEntry(_)));
    }

    #[test]
    fn enforces_file_cap() {
        let limits = ArchiveLimits {
            max_files: 1,
            ..Default::default()
        };
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buffer);
            for name in ["a.txt", "b.txt"] {
                writer
                    .start_file(name, SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(b"x").unwrap();
            }
            writer.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let err = extract_archive(&buffer.into_inner(), dir.path(), &limits).unwrap_err();
        assert!(matches!(err, PackageError::TooLarge(_)));
    }
}
