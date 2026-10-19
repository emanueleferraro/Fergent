//! Offline verification (SubTask 22.2).
//!
//! With `FERGENTS_OFFLINE=1` set, the whole Phase 1 flow must run without
//! touching the network: brain create, open, index (reindex), search
//! (`search.query`), graph (`graph.query`), and a multi-brain operation
//! (duplicate + list). This test keeps a single `#[test]` so the process-wide
//! environment variable cannot race with another test in the same binary.
//!
//! `std::env::set_var` is safe in edition 2021.

use std::path::Path;

use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::{
    BrainCreateParams, BrainDuplicateParams, DocWriteParams, GraphQueryParams, SearchQueryParams,
};

#[test]
fn full_phase_one_flow_succeeds_offline() {
    // --- offline flag ------------------------------------------------------
    std::env::set_var("FERGENTS_OFFLINE", "1");
    assert!(
        fergents_storage::offline(),
        "offline() must be true while FERGENTS_OFFLINE is set"
    );

    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("FERGENTS_HOME", dir.path());

    let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let kernel = Kernel::start(&KernelConfig {
        data_dir: fergents_core::default_data_dir(),
        migrations_dir,
        event_capacity: 64,
    })
    .unwrap();

    // --- brain.create ------------------------------------------------------
    let brain_path = dir.path().join("offline-brain");
    let created = kernel
        .brains()
        .create(&BrainCreateParams {
            color: None,
            icon: None,
            name: "Offline".to_string(),
            path: Some(brain_path.to_string_lossy().into_owned()),
            template: Some("personal".to_string()),
        })
        .unwrap();
    let brain_id = created.brain.brain_id.clone();
    assert!(!brain_id.is_empty(), "brain.create returned an empty id");

    // --- index (doc.write runs the incremental index) ----------------------
    const TOKEN: &str = "zephyrianquokka";
    let write = kernel
        .brains()
        .write_note(&DocWriteParams {
            document_id: "notes/offline.md".to_string(),
            content: format!("# Offline\n\nThe {TOKEN} protocol converges here.\n"),
        })
        .unwrap();
    assert_eq!(write.document.path, "notes/offline.md");

    // --- brain.open (incremental index) ------------------------------------
    assert_eq!(
        kernel.brains().open(&brain_id).unwrap().brain.brain_id,
        brain_id
    );

    // --- brain.reindex -----------------------------------------------------
    assert_eq!(
        kernel.brains().reindex(&brain_id).unwrap().brain_id,
        brain_id
    );

    // --- search.query ------------------------------------------------------
    let search = kernel
        .brains()
        .search_query(&SearchQueryParams {
            brains: None,
            explain: None,
            filters: None,
            group_by: None,
            mode: None,
            query: TOKEN.to_string(),
            rerank: None,
            top_k: Some(10),
        })
        .unwrap();
    assert!(
        !search.results.is_empty(),
        "offline search.query returned no results"
    );

    // --- graph.query -------------------------------------------------------
    let graph = kernel
        .brains()
        .graph_query(&GraphQueryParams {
            brain: Some(brain_id.clone()),
            language: None,
            query: "?[count(id)] := *entity{id}".to_string(),
        })
        .unwrap();
    assert!(
        !graph.rows.is_empty(),
        "offline graph.query returned no rows"
    );

    // --- multi-brain: duplicate + list -------------------------------------
    let duplicate = kernel
        .brains()
        .duplicate(&BrainDuplicateParams {
            brain_id: brain_id.clone(),
            include_attachments: None,
            name: Some("Offline copy".to_string()),
            path: Some(
                dir.path()
                    .join("offline-copy")
                    .to_string_lossy()
                    .into_owned(),
            ),
            rewrite_links: None,
        })
        .unwrap();
    assert!(
        !duplicate.brain.brain_id.is_empty(),
        "brain.duplicate returned an empty id"
    );
    let list = kernel.brains().brains(false).unwrap();
    assert_eq!(list.brains.len(), 2, "expected two registered brains");

    // --- cleanup -----------------------------------------------------------
    std::env::remove_var("FERGENTS_OFFLINE");
    std::env::remove_var("FERGENTS_HOME");
}
