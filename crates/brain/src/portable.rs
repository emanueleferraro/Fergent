//! `.fergbrain` export and import (`BRAIN.md` §14).
//!
//! A `.fergbrain` archive is a zip containing `brain.toml`, `vault/`, and a
//! `manifest.json` with a SHA-256 checksum per file. Regenerable files
//! (`index.sqlite*`, `graph.db*`, `.fergents/`) and OS noise are excluded by
//! default and rebuilt on import.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::config::BrainConfig;
use crate::error::BrainError;

/// The manifest file name inside a `.fergbrain` archive.
pub const MANIFEST_NAME: &str = "manifest.json";

/// One file recorded in an archive manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Vault-relative path (with `/` separators).
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Lowercase hex SHA-256 of the file contents.
    pub sha256: String,
}

/// The `.fergbrain` manifest (`BRAIN.md` §14.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The brain id, when `brain.toml` was present at export time.
    pub brain_id: Option<String>,
    /// The brain name, when `brain.toml` was present at export time.
    pub name: Option<String>,
    /// Export time (ms since the Unix epoch).
    pub exported_at: i64,
    /// The archived files.
    pub files: Vec<ManifestEntry>,
}

/// The summary of an import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    /// The imported brain's id (from `brain.toml`).
    pub brain_id: String,
    /// The imported brain's name.
    pub name: String,
    /// The directory the brain was extracted to.
    pub brain_dir: PathBuf,
}

/// Whether a vault-relative path is excluded from a `.fergbrain` archive.
pub fn is_excluded(rel: &str) -> bool {
    if rel == ".fergents" || rel.starts_with(".fergents/") {
        return true;
    }
    let name = rel.rsplit('/').next().unwrap_or(rel);
    matches!(name, ".DS_Store" | "Thumbs.db")
        || name == "index.sqlite"
        || name == "graph.db"
        || name.starts_with("index.sqlite-")
        || name.starts_with("graph.db-")
}

/// Export `brain_dir` to a `.fergbrain` archive at `archive_path`.
pub fn export(brain_dir: &Path, archive_path: &Path) -> Result<Manifest, BrainError> {
    let config = BrainConfig::load(brain_dir).ok();
    let mut manifest = Manifest {
        brain_id: config.as_ref().map(|c| c.brain.id.clone()),
        name: config.as_ref().map(|c| c.brain.name.clone()),
        exported_at: now_ms(),
        files: Vec::new(),
    };

    if let Some(parent) = archive_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| BrainError::io(parent, e))?;
        }
    }
    let file = std::fs::File::create(archive_path).map_err(|e| BrainError::io(archive_path, e))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for (rel, abs) in collect_files(brain_dir)? {
        if is_excluded(&rel) {
            continue;
        }
        let bytes = std::fs::read(&abs).map_err(|e| BrainError::io(&abs, e))?;
        manifest.files.push(ManifestEntry {
            path: rel.clone(),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(&bytes)),
        });
        zip.start_file(&rel, options)
            .map_err(|e| BrainError::Archive(e.to_string()))?;
        zip.write_all(&bytes)
            .map_err(|e| BrainError::Archive(e.to_string()))?;
    }

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    zip.start_file(MANIFEST_NAME, options)
        .map_err(|e| BrainError::Archive(e.to_string()))?;
    zip.write_all(&manifest_bytes)
        .map_err(|e| BrainError::Archive(e.to_string()))?;
    zip.finish()
        .map_err(|e| BrainError::Archive(e.to_string()))?;
    Ok(manifest)
}

/// Import a `.fergbrain` archive into `target_dir`, validating checksums and
/// `brain.toml`.
pub fn import(archive_path: &Path, target_dir: &Path) -> Result<ImportSummary, BrainError> {
    let file = std::fs::File::open(archive_path).map_err(|e| BrainError::io(archive_path, e))?;
    let mut zip = ZipArchive::new(file).map_err(|e| BrainError::Archive(e.to_string()))?;

    let manifest: Manifest = {
        let mut entry = zip
            .by_name(MANIFEST_NAME)
            .map_err(|_| BrainError::Archive(format!("missing {MANIFEST_NAME}")))?;
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| BrainError::Archive(e.to_string()))?;
        serde_json::from_slice(&buf).map_err(|e| BrainError::Archive(e.to_string()))?
    };

    let expected: BTreeMap<&str, &ManifestEntry> = manifest
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();

    std::fs::create_dir_all(target_dir).map_err(|e| BrainError::io(target_dir, e))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| BrainError::Archive(e.to_string()))?;
        let name = entry.name().to_string();
        if name == MANIFEST_NAME || name.ends_with('/') {
            continue;
        }
        let out = safe_join(target_dir, &name)?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| BrainError::Archive(e.to_string()))?;
        if let Some(record) = expected.get(name.as_str()) {
            let sha = hex::encode(Sha256::digest(&bytes));
            if sha != record.sha256 {
                return Err(BrainError::Archive(format!(
                    "checksum mismatch for '{name}'"
                )));
            }
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BrainError::io(parent, e))?;
        }
        std::fs::write(&out, &bytes).map_err(|e| BrainError::io(&out, e))?;
    }

    let config = BrainConfig::load(target_dir)?;
    config.validate()?;
    Ok(ImportSummary {
        brain_id: config.brain.id,
        name: config.brain.name,
        brain_dir: target_dir.to_path_buf(),
    })
}

/// Recursively collect `(vault-relative path, absolute path)` for every file in
/// `root`, with `/`-normalized relative paths.
fn collect_files(root: &Path) -> Result<Vec<(String, PathBuf)>, BrainError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| BrainError::io(&dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| BrainError::io(&dir, e))?;
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if is_excluded(&rel) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((rel, path));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Join `rel` (with `/` separators) onto `root`, rejecting traversal.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, BrainError> {
    let candidate = Path::new(rel);
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(BrainError::Archive(format!(
                    "unsafe path in archive: {rel:?}"
                )))
            }
        }
    }
    Ok(root.join(candidate))
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BrainConfig, BrainIdentity};

    fn sample_brain(dir: &Path) {
        std::fs::create_dir_all(dir.join("vault/notes")).unwrap();
        let config = BrainConfig {
            brain: BrainIdentity {
                id: "01J8K".into(),
                name: "Sample".into(),
                slug: "sample".into(),
                created_at: "2025-10-18T09:00:00Z".into(),
                ..BrainIdentity::default()
            },
            ..BrainConfig::default()
        };
        config.save(dir).unwrap();
        std::fs::write(dir.join("vault/notes/a.md"), "# A\n").unwrap();
        std::fs::write(dir.join("vault/index.sqlite"), b"regenerable").unwrap();
    }

    #[test]
    fn export_then_import_round_trips_and_excludes_regenerable_files() {
        let src = tempfile::tempdir().unwrap();
        sample_brain(src.path());
        let archive = src.path().join("brain.fergbrain");

        let manifest = export(src.path(), &archive).unwrap();
        assert_eq!(manifest.brain_id.as_deref(), Some("01J8K"));
        let paths: Vec<_> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"brain.toml"));
        assert!(paths.contains(&"vault/notes/a.md"));
        assert!(!paths.iter().any(|p| p.ends_with("index.sqlite")));

        let dest = tempfile::tempdir().unwrap();
        let target = dest.path().join("restored");
        let summary = import(&archive, &target).unwrap();
        assert_eq!(summary.brain_id, "01J8K");
        assert_eq!(summary.name, "Sample");
        assert!(target.join("vault/notes/a.md").is_file());
        assert!(!target.join("vault/index.sqlite").exists());
    }

    #[test]
    fn import_rejects_a_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("tampered.fergbrain");
        let config = BrainConfig {
            brain: BrainIdentity {
                id: "01J8K".into(),
                name: "Sample".into(),
                slug: "sample".into(),
                created_at: "2025-10-18T09:00:00Z".into(),
                ..BrainIdentity::default()
            },
            ..BrainConfig::default()
        };
        let toml = config.to_toml_string().unwrap();

        let file = std::fs::File::create(&archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("vault/notes/a.md", options).unwrap();
        zip.write_all(b"# A\n").unwrap();
        zip.start_file("brain.toml", options).unwrap();
        zip.write_all(toml.as_bytes()).unwrap();
        // The manifest claims a checksum that does not match the note's bytes.
        let manifest = Manifest {
            brain_id: Some("01J8K".into()),
            name: Some("Sample".into()),
            exported_at: 0,
            files: vec![ManifestEntry {
                path: "vault/notes/a.md".into(),
                size: 4,
                sha256: "deadbeef".into(),
            }],
        };
        zip.start_file(MANIFEST_NAME, options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap();

        let dest = tempfile::tempdir().unwrap();
        let err = import(&archive, &dest.path().join("restored")).unwrap_err();
        assert!(matches!(err, BrainError::Archive(_)));
    }
}
