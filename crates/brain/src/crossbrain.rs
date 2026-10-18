//! Cross-brain links and search scope (`BRAIN.md` §8.2, §8.3, §13).
//!
//! A link target may be same-brain (`[[notes/foo]]`), brain-scoped
//! (`[[brain:research/notes/foo]]`), or by id (`[[id:01J8K...]]`). Resolution
//! succeeds only when the target brain is attached; otherwise the link is
//! reported as unresolved.

use std::path::PathBuf;

/// A parsed wiki-link target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A same-brain link: `[[notes/foo]]`.
    SameBrain(String),
    /// A brain-scoped link: `[[brain:<alias>/<path>]]`.
    BrainScoped {
        /// The target brain's alias or id.
        brain: String,
        /// The vault-relative path within the target brain.
        path: String,
    },
    /// A link by stable brain id: `[[id:<ulid>]]`.
    ById(String),
}

/// Parse the inner text of a wiki link into a [`LinkTarget`].
pub fn parse_target(target: &str) -> LinkTarget {
    if let Some(rest) = target.strip_prefix("brain:") {
        match rest.split_once('/') {
            Some((brain, path)) => LinkTarget::BrainScoped {
                brain: brain.to_string(),
                path: path.to_string(),
            },
            None => LinkTarget::BrainScoped {
                brain: rest.to_string(),
                path: String::new(),
            },
        }
    } else if let Some(rest) = target.strip_prefix("id:") {
        LinkTarget::ById(rest.to_string())
    } else {
        LinkTarget::SameBrain(target.to_string())
    }
}

/// A brain attached to the current project, available for cross-brain links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The brain's stable id.
    pub brain_id: String,
    /// The alias used in `[[brain:<alias>/...]]` links (e.g. the slug).
    pub alias: String,
    /// The brain folder on disk.
    pub path: PathBuf,
}

/// The outcome of resolving a link target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The link resolved to an absolute path.
    Resolved(PathBuf),
    /// The target brain is not attached (or the id is unknown).
    Unresolved,
}

/// Resolve `target` against the current brain's vault and the attached brains.
pub fn resolve(
    target: &LinkTarget,
    current_vault: &std::path::Path,
    attached: &[Attachment],
) -> Resolution {
    match target {
        LinkTarget::SameBrain(path) => Resolution::Resolved(current_vault.join(path)),
        LinkTarget::ById(id) => find(attached, id)
            .map(|a| Resolution::Resolved(a.path.clone()))
            .unwrap_or(Resolution::Unresolved),
        LinkTarget::BrainScoped { brain, path } => find(attached, brain)
            .map(|a| Resolution::Resolved(a.path.join(path)))
            .unwrap_or(Resolution::Unresolved),
    }
}

fn find<'a>(attached: &'a [Attachment], key: &str) -> Option<&'a Attachment> {
    attached
        .iter()
        .find(|a| a.brain_id == key || a.alias == key)
}

/// The brain ids a cross-brain search should target (all attached brains),
/// deduplicated and sorted.
pub fn search_scope(attached: &[Attachment]) -> Vec<String> {
    let mut ids: Vec<String> = attached.iter().map(|a| a.brain_id.clone()).collect();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn attached() -> Vec<Attachment> {
        vec![Attachment {
            brain_id: "01J8L".into(),
            alias: "research".into(),
            path: PathBuf::from("/brains/research"),
        }]
    }

    #[test]
    fn parses_all_target_forms() {
        assert_eq!(
            parse_target("notes/foo"),
            LinkTarget::SameBrain("notes/foo".into())
        );
        assert_eq!(
            parse_target("brain:research/notes/foo"),
            LinkTarget::BrainScoped {
                brain: "research".into(),
                path: "notes/foo".into()
            }
        );
        assert_eq!(parse_target("id:01J8L"), LinkTarget::ById("01J8L".into()));
    }

    #[test]
    fn resolves_attached_and_reports_unresolved() {
        let current = Path::new("/brains/personal/vault");
        assert_eq!(
            resolve(&parse_target("notes/a"), current, &attached()),
            Resolution::Resolved(PathBuf::from("/brains/personal/vault/notes/a"))
        );
        assert_eq!(
            resolve(
                &parse_target("brain:research/notes/x"),
                current,
                &attached()
            ),
            Resolution::Resolved(PathBuf::from("/brains/research/notes/x"))
        );
        assert_eq!(
            resolve(&parse_target("id:01J8L"), current, &attached()),
            Resolution::Resolved(PathBuf::from("/brains/research"))
        );
        assert_eq!(
            resolve(&parse_target("brain:other/notes/x"), current, &attached()),
            Resolution::Unresolved
        );
        assert_eq!(
            resolve(&parse_target("id:missing"), current, &attached()),
            Resolution::Unresolved
        );
    }

    #[test]
    fn search_scope_is_the_attached_ids() {
        assert_eq!(search_scope(&attached()), vec!["01J8L".to_string()]);
    }
}
