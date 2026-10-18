//! Integration tests over fixture documents covering every flavor construct.

use fergents_markdown::{parse, LinkKind, MathKind};

const FULL: &str = include_str!("fixtures/full.md");
const EDGE: &str = include_str!("fixtures/code_and_edge_cases.md");

#[test]
fn full_fixture_frontmatter_is_preserved_with_unknown_keys() {
    let doc = parse(FULL);
    let fm = doc.frontmatter.expect("frontmatter");
    assert!(fm.error.is_none(), "unexpected error: {:?}", fm.error);

    let parsed = fm.parsed.expect("parsed YAML");
    assert_eq!(parsed["title"], "Full Flavor");
    // Unknown keys are preserved verbatim.
    assert_eq!(parsed["custom_key"], "keep-me");
    assert_eq!(parsed["nested"]["a"], 1);
    assert_eq!(parsed["nested"]["b"][1], "y");
}

#[test]
fn full_fixture_body_excludes_frontmatter() {
    let doc = parse(FULL);
    assert!(doc.body.starts_with("\n# Heading"));
    assert!(!doc.body.contains("custom_key"));
}

#[test]
fn full_fixture_extracts_every_link_kind() {
    let doc = parse(FULL);
    let targets: Vec<_> = doc
        .links
        .iter()
        .map(|l| (l.kind, l.target.clone()))
        .collect();

    assert_eq!(doc.links.len(), 7, "links: {targets:?}");
    assert_eq!(doc.links[0].kind, LinkKind::Wiki);
    assert_eq!(doc.links[0].target, "Note");
    assert_eq!(doc.links[0].display, None);

    assert_eq!(doc.links[1].target, "Note");
    assert_eq!(doc.links[1].display.as_deref(), Some("the label"));

    assert_eq!(doc.links[2].target, "Note");
    assert_eq!(doc.links[2].heading.as_deref(), Some("Section"));

    assert_eq!(doc.links[3].target, "Note");
    assert_eq!(doc.links[3].block_id.as_deref(), Some("blk-1"));

    assert_eq!(doc.links[4].kind, LinkKind::Embed);
    assert_eq!(doc.links[4].target, "diagram.png");
    assert_eq!(doc.links[5].kind, LinkKind::Embed);
    assert_eq!(doc.links[5].target, "Other Note");

    assert_eq!(doc.links[6].kind, LinkKind::Markdown);
    assert_eq!(doc.links[6].target, "https://example.com/docs");
    assert_eq!(doc.links[6].display.as_deref(), Some("markdown link"));
}

#[test]
fn full_fixture_extracts_tags_and_block_refs() {
    let doc = parse(FULL);
    assert_eq!(doc.tags, vec!["alpha", "nested/beta", "v2"]);
    assert_eq!(doc.block_refs, vec!["ref-42"]);
}

#[test]
fn full_fixture_extracts_callouts_math_and_mermaid() {
    let doc = parse(FULL);
    assert_eq!(doc.callouts.len(), 1);
    assert_eq!(doc.callouts[0].kind, "warning");
    assert_eq!(doc.callouts[0].title.as_deref(), Some("Watch out"));

    assert_eq!(doc.math.len(), 2);
    assert_eq!(doc.math[0].kind, MathKind::Inline);
    assert_eq!(doc.math[0].content, "E = mc^2");
    assert_eq!(doc.math[1].kind, MathKind::Block);
    assert!(doc.math[1].content.contains("\\int_0^1"));

    assert_eq!(doc.mermaid.len(), 1);
    assert!(doc.mermaid[0].code.contains("A-->B"));
}

#[test]
fn links_and_tags_inside_code_and_html_are_ignored() {
    let doc = parse(EDGE);
    assert_eq!(doc.links.len(), 1, "links: {:?}", doc.links);
    assert_eq!(doc.links[0].target, "kept");
    assert_eq!(doc.tags, vec!["kept-tag"]);
    assert!(
        doc.block_refs.is_empty(),
        "block_refs: {:?}",
        doc.block_refs
    );
}
