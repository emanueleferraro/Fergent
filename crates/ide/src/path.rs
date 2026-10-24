//! Path containment for project-relative operations (spec Appendix E).

use std::path::{Component, Path, PathBuf};

use crate::error::ProjectError;

/// Resolve a project-relative path, rejecting absolute paths, `..` segments,
/// and symlink escapes.
///
/// The nearest existing ancestor of the candidate is canonicalised and must
/// stay inside the (already canonical) project root, so a symlinked directory
/// cannot be used to reach outside the workspace.
pub fn contained(root: &Path, rel: &str) -> Result<PathBuf, ProjectError> {
    let rel = rel.trim();
    if rel.is_empty() || rel == "." {
        return Ok(root.to_path_buf());
    }
    let rel_path = Path::new(rel);
    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(ProjectError::Escape(rel.to_string())),
        }
    }

    let candidate = root.join(rel_path);
    let mut probe = candidate.as_path();
    loop {
        if probe.exists() {
            let canonical = std::fs::canonicalize(probe)?;
            if !canonical.starts_with(root) {
                return Err(ProjectError::Escape(rel.to_string()));
            }
            return Ok(candidate);
        }
        match probe.parent() {
            Some(parent) if parent.starts_with(root) => probe = parent,
            _ => return Err(ProjectError::Escape(rel.to_string())),
        }
    }
}

/// Express `path` relative to `root` with forward slashes.
pub fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
