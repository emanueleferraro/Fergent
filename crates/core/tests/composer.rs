//! Composer and Inline AI tests (tasks 3.12 and 3.13).
//!
//! The model is replaced by a deterministic client, so these tests cover the
//! kernel's own logic: diffing, approval, guarded writes, cancellation, and
//! the quick-action path through the Composer.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use fergents_core::{
    CompleteRequest, CompletionClient, ComposerError, ComposerService, InlineError, InlineService,
    ModelResolver, ProposalRequest,
};
use fergents_events::EventBus;
use fergents_ide::{FileService, ProjectService};
use fergents_llm::ChatRequest;
use fergents_protocol::ComposerState;
use fergents_reversibility::ReversibilityService;
use fergents_storage::{SqliteStorage, Storage};

struct StaticCompletion {
    reply: String,
}

impl CompletionClient for StaticCompletion {
    fn complete<'a>(
        &'a self,
        _model: &'a str,
        _request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        let reply = self.reply.clone();
        Box::pin(async move { Ok(reply) })
    }
}

struct Stack {
    root: PathBuf,
    project_id: String,
    files: Arc<FileService>,
    composer: Arc<ComposerService>,
    inline: Arc<InlineService>,
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

fn stack(dir: &Path, reply: &str) -> Stack {
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open(&dir.join("global.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 64));
    let projects = Arc::new(ProjectService::new(storage.clone(), bus.clone()));
    let reversibility = Arc::new(ReversibilityService::new(
        storage,
        bus.clone(),
        dir.join("backups"),
    ));
    let files = Arc::new(FileService::new(
        projects.clone(),
        reversibility,
        bus.clone(),
    ));
    let root = dir.join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let project = projects.open(root.to_str().unwrap()).unwrap();
    projects.set_trust(&project.id, true).unwrap();
    let completion: Arc<dyn CompletionClient> = Arc::new(StaticCompletion {
        reply: reply.to_string(),
    });
    let model_for: ModelResolver = Arc::new(|_| Some("mock-1".to_string()));
    let composer = Arc::new(ComposerService::new(
        projects.clone(),
        files.clone(),
        bus.clone(),
        completion.clone(),
        model_for.clone(),
    ));
    let inline = Arc::new(InlineService::new(
        projects,
        composer.clone(),
        completion,
        model_for,
        bus,
    ));
    Stack {
        root,
        project_id: project.id,
        files,
        composer,
        inline,
    }
}

fn proposal(project_id: &str, prompt: &str) -> ProposalRequest {
    ProposalRequest {
        project_id: project_id.to_string(),
        prompt: prompt.to_string(),
        ..ProposalRequest::default()
    }
}

#[tokio::test]
async fn proposal_diffs_and_applies_the_accepted_hunks() {
    let temp = tempfile::tempdir().unwrap();
    let reply = r#"{"message":"Rename the middle value","files":[{"path":"src/a.ts","content":"one\nTWO\nthree\n"}]}"#;
    let stack = stack(temp.path(), reply);
    stack
        .files
        .write(&stack.project_id, "src/a.ts", "one\ntwo\nthree\n", None)
        .unwrap();

    let run = stack
        .composer
        .propose(proposal(&stack.project_id, "uppercase two"))
        .await
        .unwrap();

    assert_eq!(run.message, "Rename the middle value");
    assert_eq!(run.state, ComposerState::Proposed);
    assert_eq!(run.files.len(), 1);
    assert_eq!(run.files[0].hunks.len(), 1);
    assert_eq!(run.additions, 1);
    assert_eq!(run.deletions, 1);

    let (applied_run, applied) = stack
        .composer
        .apply(&run.id, &["src/a.ts:0".to_string()])
        .unwrap();

    assert_eq!(applied_run.state, ComposerState::Applied);
    assert_eq!(applied.len(), 1);
    assert_eq!(
        std::fs::read_to_string(stack.root.join("src/a.ts")).unwrap(),
        "one\nTWO\nthree\n"
    );
}

#[tokio::test]
async fn rejected_hunks_keep_the_original_lines() {
    let temp = tempfile::tempdir().unwrap();
    let original = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    let proposed = "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nNINE\nten\n";
    let reply = format!(
        "{{\"message\":\"two edits\",\"files\":[{{\"path\":\"src/a.ts\",\"content\":{}}}]}}",
        serde_json::json!(proposed)
    );
    let stack = stack(temp.path(), &reply);
    stack
        .files
        .write(&stack.project_id, "src/a.ts", original, None)
        .unwrap();

    let run = stack
        .composer
        .propose(proposal(&stack.project_id, "two edits"))
        .await
        .unwrap();
    assert_eq!(run.files[0].hunks.len(), 2);

    stack
        .composer
        .apply(&run.id, &["src/a.ts:0".to_string()])
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(stack.root.join("src/a.ts")).unwrap(),
        "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n"
    );
}

#[tokio::test]
async fn rejecting_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let reply = r#"{"message":"rename","files":[{"path":"src/a.ts","content":"changed\n"}]}"#;
    let stack = stack(temp.path(), reply);
    stack
        .files
        .write(&stack.project_id, "src/a.ts", "original\n", None)
        .unwrap();
    let run = stack
        .composer
        .propose(proposal(&stack.project_id, "rename"))
        .await
        .unwrap();

    let rejected = stack.composer.reject(&run.id).unwrap();

    assert_eq!(rejected.state, ComposerState::Rejected);
    assert_eq!(
        std::fs::read_to_string(stack.root.join("src/a.ts")).unwrap(),
        "original\n"
    );
    assert!(matches!(
        stack.composer.apply(&run.id, &["src/a.ts:0".to_string()]),
        Err(ComposerError::NotProposed(_, _))
    ));
}

#[tokio::test]
async fn absolute_paths_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let reply = r#"{"message":"escape","files":[{"path":"/etc/passwd","content":"nope"}]}"#;
    let stack = stack(temp.path(), reply);

    let result = stack
        .composer
        .propose(proposal(&stack.project_id, "escape"))
        .await;

    assert!(matches!(result, Err(ComposerError::InvalidProposal(_))));
}

#[tokio::test]
async fn a_file_changed_after_the_proposal_is_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let reply = r#"{"message":"rename","files":[{"path":"src/a.ts","content":"proposed\n"}]}"#;
    let stack = stack(temp.path(), reply);
    stack
        .files
        .write(&stack.project_id, "src/a.ts", "original\n", None)
        .unwrap();
    let run = stack
        .composer
        .propose(proposal(&stack.project_id, "rename"))
        .await
        .unwrap();

    std::fs::write(stack.root.join("src/a.ts"), "external\n").unwrap();
    let result = stack.composer.apply(&run.id, &["src/a.ts:0".to_string()]);

    assert!(matches!(result, Err(ComposerError::Apply(_))));
    assert_eq!(
        std::fs::read_to_string(stack.root.join("src/a.ts")).unwrap(),
        "external\n"
    );
    assert_eq!(
        stack.composer.get(&run.id).unwrap().state,
        ComposerState::Failed
    );
}

#[tokio::test]
async fn new_files_are_created_by_apply() {
    let temp = tempfile::tempdir().unwrap();
    let reply =
        r#"{"message":"add","files":[{"path":"src/new.ts","content":"export const x = 1;\n"}]}"#;
    let stack = stack(temp.path(), reply);
    let run = stack
        .composer
        .propose(proposal(&stack.project_id, "add a file"))
        .await
        .unwrap();
    assert_eq!(run.files[0].original, "");

    stack
        .composer
        .apply(&run.id, &["src/new.ts:0".to_string()])
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(stack.root.join("src/new.ts")).unwrap(),
        "export const x = 1;\n"
    );
}

#[tokio::test]
async fn applying_nothing_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let reply = r#"{"message":"rename","files":[{"path":"src/a.ts","content":"changed\n"}]}"#;
    let stack = stack(temp.path(), reply);
    stack
        .files
        .write(&stack.project_id, "src/a.ts", "original\n", None)
        .unwrap();
    let run = stack
        .composer
        .propose(proposal(&stack.project_id, "rename"))
        .await
        .unwrap();

    assert!(matches!(
        stack.composer.apply(&run.id, &[]),
        Err(ComposerError::NothingAccepted)
    ));
}

#[tokio::test]
async fn completion_strips_a_markdown_fence() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), "```ts\nconst x = 1;\n```");

    let result = stack
        .inline
        .complete(CompleteRequest {
            project_id: stack.project_id.clone(),
            path: "src/a.ts".to_string(),
            prefix: "const a = 1;\n".to_string(),
            suffix: "\n".to_string(),
            language_id: Some("typescript".to_string()),
            ..CompleteRequest::default()
        })
        .await
        .unwrap();

    assert_eq!(result.suggestion, "const x = 1;");
    assert!(!result.cancelled);
    assert_eq!(result.model, "mock-1");
}

#[tokio::test]
async fn a_cancelled_completion_returns_an_empty_suggestion() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), "const x = 1;");
    assert!(stack.inline.cancel("req-1"));

    let result = stack
        .inline
        .complete(CompleteRequest {
            project_id: stack.project_id.clone(),
            path: "src/a.ts".to_string(),
            prefix: "const a = ".to_string(),
            suffix: ";".to_string(),
            request_id: Some("req-1".to_string()),
            ..CompleteRequest::default()
        })
        .await
        .unwrap();

    assert!(result.cancelled);
    assert_eq!(result.suggestion, "");
}

#[tokio::test]
async fn oversized_inline_context_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), "const x = 1;");

    let result = stack
        .inline
        .complete(CompleteRequest {
            project_id: stack.project_id.clone(),
            path: "src/a.ts".to_string(),
            prefix: "a".repeat(40_001),
            suffix: String::new(),
            ..CompleteRequest::default()
        })
        .await;

    assert!(matches!(result, Err(InlineError::ContextTooLarge)));
}

#[tokio::test]
async fn explain_returns_a_message_and_no_files() {
    let temp = tempfile::tempdir().unwrap();
    let reply = r#"{"message":"It adds two numbers.","files":[]}"#;
    let stack = stack(temp.path(), reply);
    stack
        .files
        .write(
            &stack.project_id,
            "src/a.ts",
            "fn add(a: i32, b: i32) -> i32 {\n",
            None,
        )
        .unwrap();

    let run = stack
        .inline
        .action(
            ProposalRequest {
                project_id: stack.project_id.clone(),
                paths: vec!["src/a.ts".to_string()],
                active_path: Some("src/a.ts".to_string()),
                selection: Some("fn add(a: i32, b: i32) -> i32 {".to_string()),
                ..ProposalRequest::default()
            },
            "explain",
        )
        .await
        .unwrap();

    assert_eq!(run.message, "It adds two numbers.");
    assert!(run.files.is_empty());
    assert_eq!(
        std::fs::read_to_string(stack.root.join("src/a.ts")).unwrap(),
        "fn add(a: i32, b: i32) -> i32 {\n"
    );
}
