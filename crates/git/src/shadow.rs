//! Shadow git: agent checkpoints that never touch the user's repository
//! (spec decision D9).

use std::fs;
use std::path::{Path, PathBuf};

use fergents_ide::Project;
use git2::{Commit, Index, IndexEntry, IndexTime, Repository, RepositoryInitOptions, Signature};

use crate::error::GitError;

/// Files larger than this are not checkpointed.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// A bare checkpoint repository rooted at `<project>/.fergents/shadow.git`.
pub struct ShadowRepo {
    repo: Repository,
}

impl ShadowRepo {
    /// Open or create the shadow repository for a project.
    pub fn open(project: &Project) -> Result<Self, GitError> {
        let workdir = PathBuf::from(&project.path);
        let gitdir = workdir.join(".fergents").join("shadow.git");
        fs::create_dir_all(workdir.join(".fergents"))?;
        let mut options = RepositoryInitOptions::new();
        options.bare(true).initial_head("main");
        let repo = Repository::init_opts(&gitdir, &options)?;
        Ok(Self { repo })
    }

    /// Checkpoint the working tree. Returns `None` when nothing changed.
    pub fn checkpoint(&self, workdir: &Path, label: &str) -> Result<Option<String>, GitError> {
        let mut index = Index::new()?;
        collect(&self.repo, workdir, workdir, &mut index)?;
        let tree_id = index.write_tree_to(&self.repo)?;
        let tree = self.repo.find_tree(tree_id)?;
        let parent = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        if let Some(parent) = &parent {
            if parent.tree_id() == tree_id {
                return Ok(None);
            }
        }
        let signature = Signature::now("Fergents Shadow", "shadow@fergents.local")?;
        let parents: Vec<&Commit<'_>> = parent.iter().collect();
        let id = self
            .repo
            .commit(Some("HEAD"), &signature, &signature, label, &tree, &parents)?;
        Ok(Some(id.to_string()))
    }
}

/// Add every project file (except `.fergents` and `.git`) to the index.
fn collect(
    repository: &Repository,
    root: &Path,
    directory: &Path,
    index: &mut Index,
) -> Result<(), GitError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".fergents" || name == ".git" {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect(repository, root, &path, index)?;
            continue;
        }
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| GitError::Invalid("shadow path escapes the project".to_string()))?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        let content = fs::read(&path)?;
        let id = repository.blob(&content)?;
        index.add(&IndexEntry {
            ctime: IndexTime::new(0, 0),
            mtime: IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o100644,
            uid: 0,
            gid: 0,
            file_size: content.len() as u32,
            id,
            flags: 0,
            flags_extended: 0,
            path: relative.into_bytes(),
        })?;
    }
    Ok(())
}
