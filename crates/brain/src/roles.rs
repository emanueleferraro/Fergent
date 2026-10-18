//! Brain roles in a project (`BRAIN.md` §8.1).
//!
//! A project attaches one or more brains, each with a role. Roles gate writes:
//! `primary` and `scratch` are writable, `read-only` is not.

use crate::error::BrainError;

/// The role of a brain attached to a project (`BRAIN.md` §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrainRole {
    /// The default brain for read/write. One per project.
    Primary,
    /// Searchable and readable, but not writable.
    ReadOnly,
    /// Temporary, disposable, cleared on project close.
    Scratch,
}

impl BrainRole {
    /// The lowercase role name stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::ReadOnly => "read-only",
            Self::Scratch => "scratch",
        }
    }

    /// Parse a role name, returning `None` for unknown values.
    pub fn parse_opt(value: &str) -> Option<Self> {
        match value {
            "primary" => Some(Self::Primary),
            "read-only" => Some(Self::ReadOnly),
            "scratch" => Some(Self::Scratch),
            _ => None,
        }
    }

    /// Parse a role name, rejecting unknown values.
    pub fn parse(value: &str) -> Result<Self, BrainError> {
        Self::parse_opt(value)
            .ok_or_else(|| BrainError::invalid(format!("unknown brain role: {value:?}")))
    }

    /// Whether write operations are allowed for this role.
    pub fn is_writable(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }
}

/// Reject a write to a brain whose role is not writable (`BRAIN.md` §8.1).
pub fn require_writable(role: BrainRole) -> Result<(), BrainError> {
    if role.is_writable() {
        Ok(())
    } else {
        Err(BrainError::invalid(format!(
            "brain role '{}' is read-only; writes are not allowed",
            role.as_str()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_names() {
        for role in [BrainRole::Primary, BrainRole::ReadOnly, BrainRole::Scratch] {
            assert_eq!(BrainRole::parse(role.as_str()).unwrap(), role);
        }
    }

    #[test]
    fn unknown_role_is_rejected() {
        assert!(BrainRole::parse_opt("editor").is_none());
        assert!(matches!(
            BrainRole::parse("editor"),
            Err(BrainError::Invalid(_))
        ));
    }

    #[test]
    fn only_read_only_is_not_writable() {
        assert!(BrainRole::Primary.is_writable());
        assert!(BrainRole::Scratch.is_writable());
        assert!(!BrainRole::ReadOnly.is_writable());
        assert!(require_writable(BrainRole::Primary).is_ok());
        assert!(require_writable(BrainRole::ReadOnly).is_err());
    }
}
