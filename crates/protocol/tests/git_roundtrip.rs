//! Serde round-trip tests for the Phase 3 git protocol types
//! (protocol/schema/protocol.schema.json).

use fergents_protocol::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

fn assert_roundtrip<T>(value: Value)
where
    T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug,
{
    let first: T = serde_json::from_value(value).expect("deserialize");
    let encoded = serde_json::to_value(&first).expect("serialize");
    let second: T = serde_json::from_value(encoded).expect("deserialize round-trip");
    assert_eq!(first, second);
}

fn entry() -> Value {
    json!({
        "path": "src/main.rs",
        "original_path": null,
        "kind": "modified",
        "staged": false
    })
}

fn status() -> Value {
    json!({
        "project_id": "01J9PROJECT",
        "branch": "main",
        "head": "0123456789abcdef0123456789abcdef01234567",
        "detached": false,
        "ahead": 2,
        "behind": 0,
        "clean": false,
        "entries": [entry()]
    })
}

#[test]
fn git_status_roundtrip() {
    assert_roundtrip::<GitChangeKind>(json!("conflicted"));
    assert_roundtrip::<GitStatusEntry>(entry());
    assert_roundtrip::<GitRepoStatus>(status());
    assert_roundtrip::<GitStatusResult>(json!({ "status": status() }));
    assert_roundtrip::<GitStatusParams>(json!({ "project_id": "01J9PROJECT" }));
    assert_roundtrip::<GitRepoStatus>(json!({
        "project_id": "01J9PROJECT",
        "branch": null,
        "head": null,
        "detached": true,
        "ahead": 0,
        "behind": 0,
        "clean": true,
        "entries": []
    }));
}

#[test]
fn git_diff_roundtrip() {
    let line = json!({
        "kind": "add",
        "old_line": null,
        "new_line": 2,
        "content": "two"
    });
    let hunk = json!({
        "old_start": 1,
        "old_lines": 1,
        "new_start": 1,
        "new_lines": 2,
        "header": "@@ -1 +1,2 @@",
        "lines": [line.clone()]
    });
    let file = json!({
        "path": "a.txt",
        "original_path": null,
        "binary": false,
        "hunks": [hunk.clone()]
    });
    assert_roundtrip::<GitDiffLine>(line);
    assert_roundtrip::<GitDiffHunk>(hunk);
    assert_roundtrip::<GitFileDiff>(file.clone());
    assert_roundtrip::<GitDiffResult>(json!({ "files": [file] }));
    assert_roundtrip::<GitDiffParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "a.txt",
        "staged": true,
        "commit": null
    }));
}

#[test]
fn git_commit_and_history_roundtrip() {
    let commit = json!({
        "id": "0123456789abcdef0123456789abcdef01234567",
        "short_id": "0123456",
        "summary": "feat: add the git service",
        "trailers": ["Fergents-Agent: agent:coder", "Fergents-Session: 01J9SESSION"]
    });
    assert_roundtrip::<GitCommit>(commit.clone());
    assert_roundtrip::<GitCommitResult>(json!({ "commit": commit }));
    assert_roundtrip::<GitCommitParams>(json!({
        "project_id": "01J9PROJECT",
        "message": "feat: add the git service",
        "actor": "agent:coder",
        "session_id": "01J9SESSION",
        "allow_empty": false
    }));

    let info = json!({
        "id": "0123456789abcdef0123456789abcdef01234567",
        "short_id": "0123456",
        "author": "Test User",
        "author_email": "test@example.com",
        "time": 1792694626_i64,
        "summary": "feat: add the git service"
    });
    assert_roundtrip::<GitCommitInfo>(info.clone());
    assert_roundtrip::<GitLogResult>(json!({ "commits": [info.clone()] }));
    assert_roundtrip::<GitLogParams>(json!({
        "project_id": "01J9PROJECT",
        "limit": 20
    }));
    assert_roundtrip::<GitShowResult>(json!({
        "commit": info,
        "message": "feat: add the git service\n\nBody.",
        "files": [{
            "path": "src/git/service.rs",
            "original_path": null,
            "additions": 12,
            "deletions": 0
        }]
    }));
}

#[test]
fn git_branch_and_conflict_roundtrip() {
    let branch = json!({
        "name": "feat/git",
        "is_head": true,
        "upstream": "origin/feat/git",
        "ahead": 1,
        "behind": 0
    });
    assert_roundtrip::<GitBranch>(branch.clone());
    assert_roundtrip::<GitBranchResult>(json!({ "branch": branch.clone() }));
    assert_roundtrip::<GitBranchesResult>(json!({ "branches": [branch] }));
    assert_roundtrip::<GitBranchCreateParams>(json!({
        "project_id": "01J9PROJECT",
        "name": "feat/git",
        "from": null,
        "checkout": true,
        "actor": "user"
    }));
    assert_roundtrip::<GitBranchSwitchParams>(json!({
        "project_id": "01J9PROJECT",
        "name": "feat/git",
        "actor": null
    }));

    let conflict = json!({
        "path": "a.txt",
        "has_base": true,
        "has_ours": true,
        "has_theirs": true
    });
    assert_roundtrip::<GitConflict>(conflict.clone());
    assert_roundtrip::<GitConflictsResult>(json!({ "conflicts": [conflict] }));
    assert_roundtrip::<GitResolveParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "a.txt",
        "strategy": "ours",
        "actor": "user"
    }));
    assert_roundtrip::<GitResolveResult>(json!({ "resolved": true }));
}

#[test]
fn git_paths_blame_and_remote_roundtrip() {
    assert_roundtrip::<GitPathsParams>(json!({
        "project_id": "01J9PROJECT",
        "paths": ["a.txt", "src/main.rs"],
        "actor": "user"
    }));
    assert_roundtrip::<GitStageResult>(json!({ "paths": 2 }));
    assert_roundtrip::<GitDiscardResult>(json!({
        "backup": "/work/repo/.fergents/discard/01J9BACKUP"
    }));

    let line = json!({
        "line": 1,
        "commit": "0123456789abcdef0123456789abcdef01234567",
        "short_commit": "0123456",
        "author": "Test User",
        "time": 1792694626_i64,
        "content": "fn main() {}"
    });
    assert_roundtrip::<GitBlameLine>(line.clone());
    assert_roundtrip::<GitBlameResult>(json!({ "lines": [line] }));
    assert_roundtrip::<GitBlameParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs"
    }));

    assert_roundtrip::<GitRemoteParams>(json!({
        "project_id": "01J9PROJECT",
        "remote": "origin",
        "actor": "user"
    }));
    assert_roundtrip::<GitRemoteResult>(json!({
        "operation": "push",
        "output": "Everything up-to-date"
    }));
}
