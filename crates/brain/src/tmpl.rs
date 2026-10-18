//! Built-in brain templates (`BRAIN.md` §7.1).
//!
//! A template is a set of seed files written into a freshly created vault.
//! `blank` writes nothing beyond the standard folders; the others add a small
//! starting scaffold. Richer template content is a documents/assets concern.

use crate::error::BrainError;

/// A built-in brain template (`BRAIN.md` §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrainTemplate {
    /// Empty vault, minimal config.
    Blank,
    /// Daily notes, journal, people, projects, tasks.
    Personal,
    /// Sources, notes, papers, annotations.
    Research,
    /// Snippets, project notes, decisions, docs.
    Coding,
    /// Shared brain for collaboration.
    Team,
}

impl BrainTemplate {
    /// The lowercase template name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Personal => "personal",
            Self::Research => "research",
            Self::Coding => "coding",
            Self::Team => "team",
        }
    }

    /// Parse a template name, rejecting unknown values.
    pub fn parse(value: &str) -> Result<Self, BrainError> {
        match value {
            "blank" => Ok(Self::Blank),
            "personal" => Ok(Self::Personal),
            "research" => Ok(Self::Research),
            "coding" => Ok(Self::Coding),
            "team" => Ok(Self::Team),
            other => Err(BrainError::invalid(format!(
                "unknown brain template: {other:?} (expected blank, personal, research, coding, or team)"
            ))),
        }
    }

    /// The default daily-note template written for this template, if any.
    fn daily_template(&self) -> Option<&'static str> {
        match self {
            Self::Blank | Self::Research | Self::Coding | Self::Team => None,
            Self::Personal => Some("# {{date}}\n\n## Journal\n\n{{cursor}}\n\n## Tasks\n\n"),
        }
    }

    /// Seed files as `(vault-relative path, content)` pairs.
    pub fn seed_files(self) -> Vec<(&'static str, &'static str)> {
        let mut files: Vec<(&'static str, &'static str)> = match self {
            Self::Blank => Vec::new(),
            Self::Personal => vec![("notes/index.md", "# Personal\n\nWelcome to your brain.\n")],
            Self::Research => vec![
                ("notes/index.md", "# Research\n\n"),
                ("notes/sources.md", "# Sources\n\n"),
            ],
            Self::Coding => vec![
                ("notes/index.md", "# Coding\n\n"),
                ("notes/snippets.md", "# Snippets\n\n"),
                ("notes/decisions.md", "# Decisions\n\n"),
            ],
            Self::Team => vec![("notes/index.md", "# Team\n\n")],
        };
        if let Some(daily) = self.daily_template() {
            files.push(("templates/daily.md", daily));
        }
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_builtin() {
        for name in ["blank", "personal", "research", "coding", "team"] {
            assert_eq!(BrainTemplate::parse(name).unwrap().as_str(), name);
        }
        assert!(matches!(
            BrainTemplate::parse("journal"),
            Err(BrainError::Invalid(_))
        ));
    }

    #[test]
    fn blank_seeds_nothing_and_personal_seeds_a_daily_template() {
        assert!(BrainTemplate::Blank.seed_files().is_empty());
        let seed = BrainTemplate::Personal.seed_files();
        assert!(seed.iter().any(|(p, _)| *p == "templates/daily.md"));
    }
}
