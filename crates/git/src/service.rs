//! The git subsystem: status, diffs, staging, commits, branches, conflicts,
//! remote operations, and shadow checkpoints (task 3.5).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fergents_events::{now_ms, EventBus};
use fergents_ide::{Project, ProjectService};
use fergents_storage::{AuditEntry, Storage};
use git2::build::CheckoutBuilder;
use git2::{BranchType, Oid, Repository, Sort};
use serde_json::json;

use crate::cli;
use crate::error::GitError;
use crate::model::{
    Branch, ChangeKind, CommitDetail, CommitInfo, CommitResult, ConflictFile, FileDiff,
    RemoteResult, RepoStatus, ResolveStrategy,
};
use crate::repo;
use crate::shadow::ShadowRepo;

/// Default number of log entries.
pub const DEFAULT_LOG_LIMIT: usize = 50;
/// Maximum number of log entries a single request may return.
pub const MAX_LOG_LIMIT: usize = 500;

/// The git subsystem.
pub struct GitService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    projects: Arc<ProjectService>,
    remote: bool,
    statuses: Mutex<HashMap<String, String>>,
}

impl GitService {
    /// Create the service. `remote` enables fetch/pull/push; the kernel sets it
    /// because `capability.git-remote` guards the dispatch layer.
    pub fn new(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        projects: Arc<ProjectService>,
        remote: bool,
    ) -> Self {
        Self {
            storage,
            bus,
            projects,
            remote,
            statuses: Mutex::new(HashMap::new()),
        }
    }

    // --- status -----------------------------------------------------------

    /// The repository status. Publishes `git.status.changed` when it changed
    /// since the last observation.
    pub fn status(&self, project_id: &str) -> Result<RepoStatus, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        let status = repo::status(&repository, project_id)?;
        self.publish_status(&status)?;
        Ok(status)
    }

    /// Diff the working tree, the index, or one commit.
    pub fn diff(
        &self,
        project_id: &str,
        path: Option<&str>,
        staged: bool,
        commit: Option<&str>,
    ) -> Result<Vec<FileDiff>, GitError> {
        let project = self.trusted(project_id)?;
        if let Some(path) = path {
            self.relative(&project, path)?;
        }
        let repository = repo::require(&project)?;
        repo::diff(&repository, path, staged, commit)
    }

    // --- staging ----------------------------------------------------------

    /// Stage paths (additions, modifications, and deletions).
    pub fn stage(
        &self,
        project_id: &str,
        paths: &[String],
        actor: &str,
    ) -> Result<usize, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        let mut index = repository.index()?;
        for path in paths {
            let rel = self.relative(&project, path)?;
            let absolute = Path::new(&project.path).join(&rel);
            if absolute.exists() {
                index.add_path(Path::new(&rel))?;
            } else {
                index.remove_path(Path::new(&rel))?;
            }
        }
        index.write()?;
        self.audit(
            actor,
            "git.stage",
            project_id,
            json!({ "paths": paths }),
            true,
            None,
        )?;
        self.status(project_id)?;
        Ok(paths.len())
    }

    /// Unstage paths, leaving the working tree untouched.
    pub fn unstage(
        &self,
        project_id: &str,
        paths: &[String],
        actor: &str,
    ) -> Result<usize, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        for path in paths {
            self.relative(&project, path)?;
        }
        let relative: Vec<String> = paths.iter().map(|path| path.replace('\\', "/")).collect();
        match repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
        {
            Some(commit) => {
                let pathspecs: Vec<&Path> = relative.iter().map(Path::new).collect();
                repository.reset_default(Some(commit.as_object()), pathspecs)?;
            }
            None => {
                let mut index = repository.index()?;
                for path in &relative {
                    let _ = index.remove_path(Path::new(path));
                }
                index.write()?;
            }
        }
        self.audit(
            actor,
            "git.unstage",
            project_id,
            json!({ "paths": paths }),
            true,
            None,
        )?;
        self.status(project_id)?;
        Ok(paths.len())
    }

    /// Discard working-tree changes after backing them up (spec D9: reversible).
    /// Returns the backup directory.
    pub fn discard(
        &self,
        project_id: &str,
        paths: &[String],
        actor: &str,
    ) -> Result<String, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        let backup = Path::new(&project.path)
            .join(".fergents")
            .join("discard")
            .join(ulid::Ulid::new().to_string());
        let mut tracked = Vec::new();
        for path in paths {
            let rel = self.relative(&project, path)?;
            let absolute = Path::new(&project.path).join(&rel);
            if absolute.is_file() {
                let target = backup.join(&rel);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&absolute, &target)?;
            }
            if self.is_tracked(&repository, &rel) {
                tracked.push(rel.clone());
            } else {
                let _ = fs::remove_file(&absolute);
            }
        }
        if !tracked.is_empty() {
            let mut checkout = CheckoutBuilder::new();
            checkout.force().update_index(false);
            for path in &tracked {
                checkout.path(Path::new(path));
            }
            repository.checkout_head(Some(&mut checkout))?;
        }
        let backup_path = backup.to_string_lossy().replace('\\', "/");
        self.audit(
            actor,
            "git.discard",
            project_id,
            json!({ "paths": paths, "backup": backup_path }),
            true,
            Some(&backup_path),
        )?;
        self.status(project_id)?;
        Ok(backup_path)
    }

    // --- commits ----------------------------------------------------------

    /// Commit the index, labelling agent and Composer commits with trailers.
    pub fn commit(
        &self,
        project_id: &str,
        message: &str,
        actor: &str,
        session_id: Option<&str>,
        allow_empty: bool,
    ) -> Result<CommitResult, GitError> {
        let project = self.trusted(project_id)?;
        if message.trim().is_empty() {
            return Err(GitError::Invalid("commit message is empty".to_string()));
        }
        let repository = repo::require(&project)?;
        let mut index = repository.index()?;
        let tree_id = index.write_tree()?;
        let tree = repository.find_tree(tree_id)?;
        let parent = repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        if !allow_empty {
            if let Some(parent) = &parent {
                if parent.tree_id() == tree_id {
                    return Err(GitError::Invalid("nothing to commit".to_string()));
                }
            }
        }
        let trailers = trailers_for(actor, session_id);
        let body = if trailers.is_empty() {
            message.trim_end().to_string()
        } else {
            format!("{}\n\n{}", message.trim_end(), trailers.join("\n"))
        };
        let signature = repository
            .signature()
            .or_else(|_| git2::Signature::now("Fergents", "fergents@localhost"))?;
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let id = repository.commit(Some("HEAD"), &signature, &signature, &body, &tree, &parents)?;
        let id = id.to_string();
        let summary = message.lines().next().unwrap_or_default().to_string();
        self.audit(
            actor,
            "git.commit",
            project_id,
            json!({ "message": message, "commit": id, "trailers": trailers }),
            true,
            Some(&id),
        )?;
        self.bus.publish(
            "git.commit.created",
            "git",
            json!({ "project_id": project_id, "commit": id, "trailers": trailers }),
        )?;
        self.status(project_id)?;
        Ok(CommitResult {
            short_id: repo::short(&id),
            id,
            summary,
            trailers,
        })
    }

    // --- branches ---------------------------------------------------------

    /// List local branches.
    pub fn branches(&self, project_id: &str) -> Result<Vec<Branch>, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        let mut branches = Vec::new();
        for entry in repository.branches(Some(BranchType::Local))? {
            let (branch, _) = entry?;
            let Some(name) = branch.name()?.map(str::to_string) else {
                continue;
            };
            let upstream = branch
                .upstream()
                .ok()
                .and_then(|upstream| upstream.name().ok().flatten().map(str::to_string));
            let (ahead, behind) = match (
                branch.get().target(),
                branch.upstream().ok().and_then(|u| u.get().target()),
            ) {
                (Some(local), Some(remote)) => repository
                    .graph_ahead_behind(local, remote)
                    .map(|(ahead, behind)| (ahead as u32, behind as u32))
                    .unwrap_or((0, 0)),
                _ => (0, 0),
            };
            branches.push(Branch {
                is_head: branch.is_head(),
                name,
                upstream,
                ahead,
                behind,
            });
        }
        branches.sort_by(|a, b| b.is_head.cmp(&a.is_head).then_with(|| a.name.cmp(&b.name)));
        Ok(branches)
    }

    /// Create a branch from a commit (or HEAD) and optionally check it out.
    pub fn create_branch(
        &self,
        project_id: &str,
        name: &str,
        from: Option<&str>,
        checkout: bool,
        actor: &str,
    ) -> Result<Branch, GitError> {
        let project = self.trusted(project_id)?;
        validate_branch(name)?;
        let repository = repo::require(&project)?;
        let commit = match from {
            Some(from) => repository.find_commit(Oid::from_str(from)?)?,
            None => repository
                .head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok())
                .ok_or_else(|| GitError::Invalid("repository has no commits".to_string()))?,
        };
        repository.branch(name, &commit, false)?;
        if checkout {
            self.checkout(&repository, name)?;
        }
        self.audit(
            actor,
            "git.branch.create",
            project_id,
            json!({ "name": name, "from": from, "checkout": checkout }),
            true,
            None,
        )?;
        self.bus.publish(
            "git.status.changed",
            "git",
            json!({ "project_id": project_id, "branch": name }),
        )?;
        Ok(Branch {
            name: name.to_string(),
            is_head: checkout,
            upstream: None,
            ahead: 0,
            behind: 0,
        })
    }

    /// Switch to an existing local branch.
    pub fn switch_branch(
        &self,
        project_id: &str,
        name: &str,
        actor: &str,
    ) -> Result<RepoStatus, GitError> {
        let project = self.trusted(project_id)?;
        validate_branch(name)?;
        let repository = repo::require(&project)?;
        repository
            .find_branch(name, BranchType::Local)
            .map_err(|_| GitError::Invalid(format!("unknown branch: {name}")))?;
        self.checkout(&repository, name)?;
        self.audit(
            actor,
            "git.branch.switch",
            project_id,
            json!({ "name": name }),
            true,
            None,
        )?;
        self.status(project_id)
    }

    // --- history ----------------------------------------------------------

    /// The commit log, newest first.
    pub fn log(&self, project_id: &str, limit: Option<usize>) -> Result<Vec<CommitInfo>, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        let limit = limit.unwrap_or(DEFAULT_LOG_LIMIT).clamp(1, MAX_LOG_LIMIT);
        let mut walk = repository.revwalk()?;
        walk.push_head()
            .map_err(|_| GitError::Invalid("repository has no commits".to_string()))?;
        walk.set_sorting(Sort::TIME)?;
        let mut entries = Vec::new();
        for oid in walk.take(limit) {
            let commit = repository.find_commit(oid?)?;
            entries.push(repo::commit_info(&commit));
        }
        Ok(entries)
    }

    /// One commit with its message and changed files.
    pub fn show(&self, project_id: &str, id: &str) -> Result<CommitDetail, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        repo::show(&repository, id)
    }

    /// Blame one tracked file.
    pub fn blame(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<Vec<crate::model::BlameLine>, GitError> {
        let project = self.trusted(project_id)?;
        let rel = self.relative(&project, path)?;
        let repository = repo::require(&project)?;
        repo::blame(&repository, &rel)
    }

    // --- conflicts --------------------------------------------------------

    /// The index conflicts.
    pub fn conflicts(&self, project_id: &str) -> Result<Vec<ConflictFile>, GitError> {
        let project = self.trusted(project_id)?;
        let repository = repo::require(&project)?;
        let index = repository.index()?;
        let mut conflicts = Vec::new();
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            let entry = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref());
            let Some(entry) = entry else {
                continue;
            };
            let path = String::from_utf8_lossy(&entry.path).replace('\\', "/");
            conflicts.push(ConflictFile {
                path,
                has_base: conflict.ancestor.is_some(),
                has_ours: conflict.our.is_some(),
                has_theirs: conflict.their.is_some(),
            });
        }
        Ok(conflicts)
    }

    /// Resolve one conflict by taking a side or accepting the working tree.
    pub fn resolve(
        &self,
        project_id: &str,
        path: &str,
        strategy: ResolveStrategy,
        actor: &str,
    ) -> Result<bool, GitError> {
        let project = self.trusted(project_id)?;
        let rel = self.relative(&project, path)?;
        let repository = repo::require(&project)?;
        let mut index = repository.index()?;
        let conflict = index.conflicts()?.filter_map(Result::ok).find(|conflict| {
            let entry = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref());
            entry
                .map(|entry| String::from_utf8_lossy(&entry.path).replace('\\', "/") == rel)
                .unwrap_or(false)
        });
        let Some(conflict) = conflict else {
            return Err(GitError::Invalid(format!("no conflict for {rel}")));
        };
        let absolute = Path::new(&project.path).join(&rel);
        if strategy != ResolveStrategy::MarkResolved {
            let blob_id = match strategy {
                ResolveStrategy::Ours => conflict.our.as_ref().map(|entry| entry.id),
                ResolveStrategy::Theirs => conflict.their.as_ref().map(|entry| entry.id),
                ResolveStrategy::MarkResolved => None,
            };
            match blob_id {
                Some(id) if !id.is_zero() => {
                    let blob = repository.find_blob(id)?;
                    if let Some(parent) = absolute.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&absolute, blob.content())?;
                }
                _ => {
                    let _ = fs::remove_file(&absolute);
                }
            }
        }
        if absolute.exists() {
            index.add_path(Path::new(&rel))?;
        } else {
            index.remove_path(Path::new(&rel))?;
        }
        index.write()?;
        self.audit(
            actor,
            "git.resolve",
            project_id,
            json!({ "path": rel, "strategy": strategy.as_str() }),
            true,
            None,
        )?;
        self.status(project_id)?;
        Ok(true)
    }

    // --- remote -----------------------------------------------------------

    /// Fetch from a remote (requires remote operations to be enabled).
    pub fn fetch(
        &self,
        project_id: &str,
        remote: Option<&str>,
        actor: &str,
    ) -> Result<RemoteResult, GitError> {
        self.remote(project_id, remote, actor, "fetch")
    }

    /// Pull with fast-forward only (never merges or rewrites history).
    pub fn pull(
        &self,
        project_id: &str,
        remote: Option<&str>,
        actor: &str,
    ) -> Result<RemoteResult, GitError> {
        self.remote(project_id, remote, actor, "pull")
    }

    /// Push to a remote. Force-push is never offered (spec decision D10).
    pub fn push(
        &self,
        project_id: &str,
        remote: Option<&str>,
        actor: &str,
    ) -> Result<RemoteResult, GitError> {
        self.remote(project_id, remote, actor, "push")
    }

    // --- shadow git -------------------------------------------------------

    /// Checkpoint the working tree into the shadow repository. Returns the
    /// checkpoint commit, or `None` when nothing changed.
    pub fn checkpoint(&self, project_id: &str, label: &str) -> Result<Option<String>, GitError> {
        let project = self.trusted(project_id)?;
        let shadow = ShadowRepo::open(&project)?;
        let id = shadow.checkpoint(Path::new(&project.path), label)?;
        if let Some(id) = &id {
            self.audit(
                "agent",
                "git.checkpoint",
                project_id,
                json!({ "label": label, "commit": id }),
                true,
                Some(id),
            )?;
        }
        Ok(id)
    }

    // --- internals --------------------------------------------------------

    /// Load a project and require that it is trusted.
    fn trusted(&self, project_id: &str) -> Result<Project, GitError> {
        Ok(self.projects.require_trusted(project_id)?)
    }

    /// Validate and normalise a project-relative path.
    fn relative(&self, project: &Project, path: &str) -> Result<String, GitError> {
        let absolute = self.projects.resolve(project, path)?;
        let root = PathBuf::from(&project.path);
        let relative = absolute
            .strip_prefix(&root)
            .map_err(|_| GitError::Invalid(format!("path escapes the project: {path}")))?;
        Ok(relative.to_string_lossy().replace('\\', "/"))
    }

    /// Whether a path is tracked by the repository.
    fn is_tracked(&self, repository: &Repository, path: &str) -> bool {
        repository
            .index()
            .ok()
            .and_then(|index| index.get_path(Path::new(path), 0))
            .is_some()
    }

    /// Check out a branch, refusing to discard uncommitted tracked changes.
    fn checkout(&self, repository: &Repository, name: &str) -> Result<(), GitError> {
        let status = repo::status(repository, "")?;
        if status
            .entries
            .iter()
            .any(|entry| entry.kind != ChangeKind::Untracked)
        {
            return Err(GitError::Invalid(
                "working tree has uncommitted changes".to_string(),
            ));
        }
        let reference = format!("refs/heads/{name}");
        repository.set_head(&reference)?;
        let mut checkout = CheckoutBuilder::new();
        checkout.force().update_index(true);
        repository
            .checkout_head(Some(&mut checkout))
            .map_err(|error| {
                GitError::Invalid(format!("cannot switch to {name}: {}", error.message()))
            })
    }

    /// Run one remote command through the CLI.
    fn remote(
        &self,
        project_id: &str,
        remote: Option<&str>,
        actor: &str,
        operation: &str,
    ) -> Result<RemoteResult, GitError> {
        if !self.remote {
            return Err(GitError::RemoteDisabled);
        }
        let project = self.trusted(project_id)?;
        let mut args: Vec<&str> = vec![operation];
        if let Some(remote) = remote {
            args.push(remote);
        }
        if operation == "pull" {
            args.push("--ff-only");
        }
        let output = cli::run(Path::new(&project.path), &args)?;
        self.audit(
            actor,
            &format!("git.{operation}"),
            project_id,
            json!({ "remote": remote }),
            true,
            None,
        )?;
        self.bus.publish(
            "git.status.changed",
            "git",
            json!({ "project_id": project_id, "operation": operation }),
        )?;
        Ok(RemoteResult {
            operation: operation.to_string(),
            output,
        })
    }

    /// Publish a status event when the status changed since last time.
    fn publish_status(&self, status: &RepoStatus) -> Result<(), GitError> {
        let signature = repo::signature(status);
        let changed = {
            let mut seen = self
                .statuses
                .lock()
                .map_err(|_| GitError::Invalid("git status lock poisoned".to_string()))?;
            match seen.get(&status.project_id) {
                Some(previous) if previous == &signature => false,
                _ => {
                    seen.insert(status.project_id.clone(), signature);
                    true
                }
            }
        };
        if !changed {
            return Ok(());
        }
        let conflicts = status
            .entries
            .iter()
            .filter(|entry| entry.kind == crate::model::ChangeKind::Conflicted)
            .count();
        self.bus.publish(
            "git.status.changed",
            "git",
            json!({
                "project_id": status.project_id,
                "branch": status.branch,
                "clean": status.clean,
                "entries": status.entries.len(),
                "conflicts": conflicts,
            }),
        )?;
        if conflicts > 0 {
            self.bus.publish(
                "git.conflict",
                "git",
                json!({ "project_id": status.project_id, "conflicts": conflicts }),
            )?;
        }
        Ok(())
    }

    /// Append an audit entry.
    fn audit(
        &self,
        actor: &str,
        action: &str,
        project_id: &str,
        args: serde_json::Value,
        reversible: bool,
        revert_ref: Option<&str>,
    ) -> Result<(), GitError> {
        self.storage.audit(&AuditEntry {
            id: &ulid::Ulid::new().to_string(),
            ts: now_ms(),
            actor_type: if actor.starts_with("agent") || actor == "composer" {
                "agent"
            } else {
                "user"
            },
            actor_id: actor,
            session_id: None,
            action,
            target: Some(project_id),
            args_json: &args.to_string(),
            result: "ok",
            reason: None,
            correlation_id: None,
            reversible,
            revert_ref,
        })?;
        Ok(())
    }
}

/// Reject branch names libgit2 would refuse anyway, with a clearer message.
fn validate_branch(name: &str) -> Result<(), GitError> {
    if name.trim().is_empty() {
        return Err(GitError::Invalid("branch name is empty".to_string()));
    }
    if !git2::Reference::is_valid_name(&format!("refs/heads/{name}")) {
        return Err(GitError::Invalid(format!("invalid branch name: {name}")));
    }
    Ok(())
}

/// The trailers a commit must carry, given its actor.
pub fn trailers_for(actor: &str, session_id: Option<&str>) -> Vec<String> {
    let agent = actor.starts_with("agent") || actor == "composer" || actor.starts_with("composer:");
    if !agent {
        return Vec::new();
    }
    let mut trailers = vec![format!("Fergents-Agent: {actor}")];
    if let Some(session) = session_id {
        trailers.push(format!("Fergents-Session: {session}"));
    }
    trailers
}

#[cfg(test)]
mod tests {
    use super::trailers_for;

    #[test]
    fn user_commits_carry_no_trailers() {
        assert!(trailers_for("user", Some("session-1")).is_empty());
        assert!(trailers_for("user", None).is_empty());
    }

    #[test]
    fn agent_and_composer_commits_are_labelled() {
        assert_eq!(
            trailers_for("agent:coder", Some("session-1")),
            vec![
                "Fergents-Agent: agent:coder".to_string(),
                "Fergents-Session: session-1".to_string()
            ]
        );
        assert_eq!(
            trailers_for("composer", None),
            vec!["Fergents-Agent: composer".to_string()]
        );
    }
}
