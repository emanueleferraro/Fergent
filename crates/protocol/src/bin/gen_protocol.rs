//! Regenerate the committed Rust protocol types from the JSON Schema.
//!
//! Usage: `cargo run -p fergents-protocol --bin gen-protocol`

use std::path::Path;
use std::process::ExitCode;

use fergents_protocol::codegen::{find_repo_root, generate_rust, SCHEMA_RELPATH};

fn main() -> ExitCode {
    match run() {
        Ok(path) => {
            println!("generated {}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<std::path::PathBuf> {
    let repo_root = find_repo_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .ok_or_else(|| anyhow::anyhow!("could not locate repository root"))?;
    let schema_json = std::fs::read_to_string(repo_root.join(SCHEMA_RELPATH))?;
    let code = generate_rust(&schema_json)?;
    let out = repo_root.join("crates/protocol/src/generated.rs");
    std::fs::write(&out, code)?;
    Ok(out)
}
