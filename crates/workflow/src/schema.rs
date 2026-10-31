//! The shipped workflow JSON Schema (spec §1.8).
//!
//! `workflow.schema.json` is the editor contract: Monaco uses it for
//! completion and red squiggles, and the desktop form generator reads it. The
//! semantic rules that JSON Schema cannot express live in [`crate::validate`].
//!
//! The schema is embedded so the kernel ships it as a single binary.

use std::sync::OnceLock;

/// The schema source, as shipped.
pub const SCHEMA_JSON: &str = include_str!("../schema/workflow.schema.json");

/// The schema `$id`, the stable URL editors key on.
pub const SCHEMA_ID: &str = "https://fergents.dev/schema/workflow.schema.json";

/// The parsed schema, parsed once on first use.
///
/// Parsing is infallible in practice: a committed test parses it, so a broken
/// schema fails CI rather than a user's save.
pub fn schema_value() -> &'static serde_json::Value {
    static SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(SCHEMA_JSON)
            .unwrap_or_else(|err| panic!("schema/workflow.schema.json is not valid JSON: {err}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_parses_and_declares_draft_2020_12() {
        let schema = schema_value();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["$id"], SCHEMA_ID);
    }

    #[test]
    fn schema_requires_the_identity_keys() {
        let required: Vec<&str> = schema_value()["required"]
            .as_array()
            .expect("required is an array")
            .iter()
            .map(|value| value.as_str().expect("required entries are strings"))
            .collect();
        assert!(required.contains(&"id"));
        assert!(required.contains(&"name"));
        assert!(required.contains(&"triggers"));
        assert!(required.contains(&"nodes"));
    }

    #[test]
    fn schema_mirrors_the_builtin_node_types() {
        let variants = schema_value()["$defs"]["nodeType"]["oneOf"][0]["enum"]
            .as_array()
            .expect("nodeType enum");
        let names: Vec<&str> = variants
            .iter()
            .map(|value| value.as_str().expect("node type names are strings"))
            .collect();
        let kinds = crate::document::NodeKind::BUILTIN;
        let builtin: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(names, builtin);
    }
}
