//! Repository helpers over `git2` (spec decision D8: vendored libgit2).

use std::fs;
use std::path::Path;

use fergents_ide::Project;
use git2::{Commit, DiffOptions, Repository, Status, StatusOptions};

use crate::error::GitError;
use crate::model::{
    BlameLine, ChangeKind, CommitDetail, CommitInfo, DiffFileSummary, DiffHunk, DiffLine, FileDiff,
    RepoStatus, StatusEntry,
};

/// Open the repository that contains the project folder.
pub fn open(project: &Project) -> Result<Repository, GitError> {
    Repository::open(&project.path).map_err(|_| GitError::NotRepository(project.path.clone()))
}

/// Open the project repository, rejecting projects that are not git-backed.
pub fn require(project: &Project) -> Result<Repository, GitError> {
    if project.vcs.as_deref() != Some("git") {
        return Err(GitError::NotRepository(project.path.clone()));
    }
    open(project)
}

/// The seven-character abbreviation of a commit id.
pub fn short(id: &str) -> String {
    id.chars().take(7).collect()
}

/// Compute the repository status.
pub fn status(repo: &Repository, project_id: &str) -> Result<RepoStatus, GitError> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    let statuses = repo.statuses(Some(&mut options))?;

    let mut entries = Vec::new();
    for entry in statuses.iter() {
        let state = entry.status();
        if state == Status::CURRENT || state.is_ignored() || state.is_empty() {
            continue;
        }
        let Some(path) = entry.path() else {
            continue;
        };
        let path = path.replace('\\', "/");
        if is_internal(&path) {
            continue;
        }
        let original = entry
            .head_to_index()
            .or_else(|| entry.index_to_workdir())
            .and_then(|delta| delta.old_file().path().map(normalise))
            .filter(|old| old != &path);

        if state.is_conflicted() {
            entries.push(StatusEntry {
                path,
                original_path: original,
                kind: ChangeKind::Conflicted,
                staged: false,
            });
            continue;
        }
        if has_index_change(state) {
            entries.push(StatusEntry {
                path: path.clone(),
                original_path: original.clone(),
                kind: staged_kind(state),
                staged: true,
            });
        }
        if has_worktree_change(state) {
            entries.push(StatusEntry {
                path,
                original_path: original,
                kind: worktree_kind(state),
                staged: false,
            });
        }
    }

    let head = repo.head().ok();
    let branch = head
        .as_ref()
        .filter(|head| head.is_branch())
        .and_then(|head| head.shorthand().map(str::to_string));
    let detached = repo.head_detached().unwrap_or(false);
    let commit = match head.as_ref() {
        Some(head) => head.target().map(|id| id.to_string()),
        None => None,
    };

    let (ahead, behind) = match branch.as_deref() {
        Some(name) => tracking(repo, name),
        None => (0, 0),
    };

    Ok(RepoStatus {
        project_id: project_id.to_string(),
        branch,
        head: commit,
        detached,
        ahead,
        behind,
        clean: entries.is_empty(),
        entries,
    })
}

/// A stable signature of a status, used to deduplicate change events.
pub fn signature(status: &RepoStatus) -> String {
    let mut parts: Vec<String> = status
        .entries
        .iter()
        .map(|entry| {
            format!(
                "{}:{}:{}",
                entry.kind.as_str(),
                if entry.staged { "i" } else { "w" },
                entry.path
            )
        })
        .collect();
    parts.sort();
    format!(
        "{}|{}|{}|{}",
        status.branch.clone().unwrap_or_default(),
        status.head.clone().unwrap_or_default(),
        status.ahead,
        parts.join(",")
    )
}

/// Diff the working tree, the index, or one commit.
pub fn diff(
    repo: &Repository,
    path: Option<&str>,
    staged: bool,
    commit: Option<&str>,
) -> Result<Vec<FileDiff>, GitError> {
    let mut options = DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_typechange(true);
    if let Some(path) = path {
        options.pathspec(path);
    }

    let diff = match commit {
        Some(id) => {
            let commit = repo.find_commit(git2::Oid::from_str(id)?)?;
            let tree = commit.tree()?;
            let parent = commit
                .parent(0)
                .ok()
                .map(|parent| parent.tree())
                .transpose()?;
            repo.diff_tree_to_tree(parent.as_ref(), Some(&tree), Some(&mut options))?
        }
        None if staged => {
            let head = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            repo.diff_tree_to_index(head.as_ref(), None, Some(&mut options))?
        }
        None => repo.diff_index_to_workdir(None, Some(&mut options))?,
    };

    let mut files = Vec::with_capacity(diff.deltas().len());
    for index in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(index) else {
            continue;
        };
        let binary = delta.flags().is_binary();
        let path = delta_path(&delta);
        if is_internal(&path) {
            continue;
        }
        let original = delta
            .old_file()
            .path()
            .map(normalise)
            .filter(|old| old != &path);
        let mut hunks = Vec::new();
        if !binary {
            if let Some(patch) = git2::Patch::from_diff(&diff, index)? {
                for hunk_index in 0..patch.num_hunks() {
                    let (hunk, count) = patch.hunk(hunk_index)?;
                    let mut lines = Vec::with_capacity(count);
                    for line_index in 0..count {
                        let line = patch.line_in_hunk(hunk_index, line_index)?;
                        let kind = match line.origin() {
                            '+' => "add",
                            '-' => "del",
                            _ => "context",
                        };
                        lines.push(DiffLine {
                            kind: kind.to_string(),
                            old_line: line.old_lineno(),
                            new_line: line.new_lineno(),
                            content: trim_newline(line.content()),
                        });
                    }
                    hunks.push(DiffHunk {
                        old_start: hunk.old_start(),
                        old_lines: hunk.old_lines(),
                        new_start: hunk.new_start(),
                        new_lines: hunk.new_lines(),
                        header: trim_newline(hunk.header()),
                        lines,
                    });
                }
            }
        }
        files.push(FileDiff {
            path,
            original_path: original,
            binary,
            hunks,
        });
    }
    Ok(files)
}

/// A commit with its message and changed files.
pub fn show(repo: &Repository, id: &str) -> Result<CommitDetail, GitError> {
    let commit = repo.find_commit(git2::Oid::from_str(id)?)?;
    let tree = commit.tree()?;
    let parent = commit
        .parent(0)
        .ok()
        .map(|parent| parent.tree())
        .transpose()?;
    let diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&tree), None)?;
    let mut files = Vec::with_capacity(diff.deltas().len());
    for index in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(index) else {
            continue;
        };
        let (additions, deletions) = match git2::Patch::from_diff(&diff, index)? {
            Some(patch) => {
                let (_, additions, deletions) = patch.line_stats()?;
                (additions as u32, deletions as u32)
            }
            None => (0, 0),
        };
        let path = delta_path(&delta);
        if is_internal(&path) {
            continue;
        }
        let original = delta
            .old_file()
            .path()
            .map(normalise)
            .filter(|old| old != &path);
        files.push(DiffFileSummary {
            path,
            original_path: original,
            additions,
            deletions,
        });
    }
    Ok(CommitDetail {
        commit: commit_info(&commit),
        message: commit.message().unwrap_or_default().to_string(),
        files,
    })
}

/// Blame one working-tree file.
pub fn blame(repo: &Repository, path: &str) -> Result<Vec<BlameLine>, GitError> {
    let blame = repo.blame_file(Path::new(path), None)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| GitError::Invalid("repository has no working tree".to_string()))?;
    let text = fs::read_to_string(workdir.join(path))?;
    let mut lines = Vec::new();
    for (index, content) in text.lines().enumerate() {
        let number = index + 1;
        let Some(hunk) = blame.get_line(number) else {
            continue;
        };
        let signature = hunk.final_signature();
        let commit = hunk.final_commit_id().to_string();
        lines.push(BlameLine {
            line: number as u32,
            short_commit: short(&commit),
            commit,
            author: signature.name().unwrap_or_default().to_string(),
            time: signature.when().seconds(),
            content: content.to_string(),
        });
    }
    Ok(lines)
}

/// Build commit metadata from a commit.
pub fn commit_info(commit: &Commit<'_>) -> CommitInfo {
    let id = commit.id().to_string();
    let author = commit.author();
    CommitInfo {
        short_id: short(&id),
        id,
        author: author.name().unwrap_or_default().to_string(),
        author_email: author.email().unwrap_or_default().to_string(),
        time: author.when().seconds(),
        summary: commit.summary().unwrap_or_default().to_string(),
    }
}

/// Whether an entry carries staged changes.
fn has_index_change(state: Status) -> bool {
    state.intersects(
        Status::INDEX_NEW
            | Status::INDEX_MODIFIED
            | Status::INDEX_DELETED
            | Status::INDEX_RENAMED
            | Status::INDEX_TYPECHANGE,
    )
}

/// Whether an entry carries working-tree changes.
fn has_worktree_change(state: Status) -> bool {
    state.intersects(
        Status::WT_NEW
            | Status::WT_MODIFIED
            | Status::WT_DELETED
            | Status::WT_RENAMED
            | Status::WT_TYPECHANGE,
    )
}

/// The kind of a staged change.
fn staged_kind(state: Status) -> ChangeKind {
    if state.is_index_new() {
        ChangeKind::Added
    } else if state.is_index_deleted() {
        ChangeKind::Deleted
    } else if state.is_index_renamed() {
        ChangeKind::Renamed
    } else if state.is_index_typechange() {
        ChangeKind::Typechange
    } else {
        ChangeKind::Modified
    }
}

/// The kind of a working-tree change.
fn worktree_kind(state: Status) -> ChangeKind {
    if state.is_wt_new() {
        ChangeKind::Untracked
    } else if state.is_wt_deleted() {
        ChangeKind::Deleted
    } else if state.is_wt_renamed() {
        ChangeKind::Renamed
    } else if state.is_wt_typechange() {
        ChangeKind::Typechange
    } else {
        ChangeKind::Modified
    }
}

/// Ahead/behind counts against the branch's upstream.
fn tracking(repo: &Repository, name: &str) -> (u32, u32) {
    let Ok(local) = repo.find_branch(name, git2::BranchType::Local) else {
        return (0, 0);
    };
    let Ok(upstream) = local.upstream() else {
        return (0, 0);
    };
    match (local.get().target(), upstream.get().target()) {
        (Some(local), Some(upstream)) => repo
            .graph_ahead_behind(local, upstream)
            .map(|(ahead, behind)| (ahead as u32, behind as u32))
            .unwrap_or((0, 0)),
        _ => (0, 0),
    }
}

/// The path of a delta, preferring the new side.
fn delta_path(delta: &git2::DiffDelta<'_>) -> String {
    let path = match delta.status() {
        git2::Delta::Deleted => delta.old_file().path(),
        _ => delta.new_file().path().or_else(|| delta.old_file().path()),
    };
    path.map(normalise).unwrap_or_default()
}

/// Normalise a repository path to forward slashes.
fn normalise(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Whether a path belongs to Fergents' own state (never shown as a change).
fn is_internal(path: &str) -> bool {
    path == ".fergents" || path.starts_with(".fergents/")
}

/// Trim a trailing newline from a diff line or header.
fn trim_newline(text: &[u8]) -> String {
    String::from_utf8_lossy(text)
        .trim_end_matches(['\n', '\r'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{is_internal, short};

    #[test]
    fn internal_paths_are_never_reported() {
        assert!(is_internal(".fergents"));
        assert!(is_internal(".fergents/discard/x"));
        assert!(!is_internal("src/main.rs"));
        assert!(!is_internal(".fergentsx/a"));
    }

    #[test]
    fn commit_ids_are_abbreviated() {
        assert_eq!(short("0123456789abcdef"), "0123456");
        assert_eq!(short("abc"), "abc");
    }
}
