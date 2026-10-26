//! Version and API compatibility rules (`PLUGINS.md` §13).

use semver::Version;

use crate::error::PackageError;

/// The API version implemented by this kernel.
pub const KERNEL_API_VERSION: u32 = 1;

/// Parse a semantic version, returning a stable package error.
pub fn parse_version(value: &str) -> Result<Version, PackageError> {
    Version::parse(value.trim()).map_err(|_| PackageError::InvalidVersion {
        value: value.to_string(),
    })
}

/// Whether a package targeting `required` can run on `current`.
///
/// The kernel supports its own API version and the previous one (N-1).
pub fn api_compatible(required: u32, current: u32) -> bool {
    required == current || (current > 0 && required + 1 == current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_current_and_previous() {
        assert!(api_compatible(1, 1));
        assert!(api_compatible(0, 1));
        assert!(!api_compatible(2, 1));
        assert!(api_compatible(1, 2));
        assert!(!api_compatible(3, 2));
    }

    #[test]
    fn versions_parse() {
        assert_eq!(parse_version("1.2.3").unwrap().major, 1);
        assert!(parse_version("not-a-version").is_err());
    }
}
