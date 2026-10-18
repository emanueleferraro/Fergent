//! Splitting and decoding YAML frontmatter.

use crate::model::Frontmatter;

/// Split a document into its YAML frontmatter (if present) and the body.
///
/// Frontmatter is recognized only when the document's very first line is a
/// `---` fence; the block runs until the next line that is exactly `---` or
/// `...`. If there is no closing fence, the whole input is treated as body.
pub(crate) fn split(input: &str) -> (Option<Frontmatter>, &str) {
    let rest = match strip_opening_fence(input) {
        Some(rest) => rest,
        None => return (None, input),
    };

    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            let yaml = &rest[..line_start];
            let body = &rest[offset..];
            return (Some(decode(yaml)), body);
        }
    }

    // No closing fence: not frontmatter.
    (None, input)
}

/// Strip a leading `---` fence and its trailing newline, if present.
fn strip_opening_fence(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("---")?;
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

/// Decode the YAML text, preserving unknown keys and never panicking.
fn decode(yaml: &str) -> Frontmatter {
    let raw = yaml.to_string();
    if raw.trim().is_empty() {
        return Frontmatter {
            raw,
            parsed: Some(serde_yaml::Value::Mapping(serde_yaml::Mapping::new())),
            error: None,
        };
    }
    match serde_yaml::from_str::<serde_yaml::Value>(&raw) {
        Ok(value) => Frontmatter {
            raw,
            parsed: Some(value),
            error: None,
        },
        Err(err) => Frontmatter {
            raw,
            parsed: None,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_and_body() {
        let input = "---\ntitle: Hello\ntags: [a, b]\n---\n\n# Body\n";
        let (fm, body) = split(input);
        let fm = fm.expect("frontmatter");
        assert_eq!(fm.raw, "title: Hello\ntags: [a, b]\n");
        assert_eq!(body, "\n# Body\n");
        assert!(fm.error.is_none());
    }

    #[test]
    fn no_frontmatter_when_no_opening_fence() {
        let input = "# Just a heading\n\n---\n";
        let (fm, body) = split(input);
        assert!(fm.is_none());
        assert_eq!(body, input);
    }

    #[test]
    fn no_frontmatter_without_closing_fence() {
        let input = "---\ntitle: Hello\nnot closed";
        let (fm, body) = split(input);
        assert!(fm.is_none());
        assert_eq!(body, input);
    }

    #[test]
    fn supports_dots_closing_fence_and_crlf() {
        let input = "---\r\ntitle: Hi\r\n...\r\nbody\r\n";
        let (fm, body) = split(input);
        assert!(fm.is_some());
        assert_eq!(body, "body\r\n");
    }

    #[test]
    fn empty_frontmatter_is_an_empty_mapping() {
        let input = "---\n---\nbody";
        let (fm, body) = split(input);
        let fm = fm.expect("frontmatter");
        assert_eq!(
            fm.parsed,
            Some(serde_yaml::Value::Mapping(Default::default()))
        );
        assert_eq!(body, "body");
    }

    #[test]
    fn malformed_yaml_is_reported_not_panicked() {
        let input = "---\ntitle: : :\n---\nbody";
        let (fm, _body) = split(input);
        let fm = fm.expect("frontmatter");
        assert!(fm.parsed.is_none());
        assert!(fm.error.is_some());
    }
}
