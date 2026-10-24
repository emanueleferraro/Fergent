//! A deterministic language server used by the tests and CI.
//!
//! It speaks the LSP base protocol over stdio and answers the handful of
//! requests the kernel issues. It is not shipped in the app.

use std::io::{self, BufReader, Write};

use serde_json::{json, Value};

fn main() {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = io::stdout();

    while let Ok(Some(message)) = fergents_lsp::read_message(&mut reader) {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let id = message.get("id").cloned();

        if method == "exit" {
            break;
        }

        let response = match (method.as_str(), id) {
            ("initialize", Some(id)) => {
                let _ = send(
                    &mut out,
                    json!({
                        "jsonrpc": "2.0",
                        "method": "window/logMessage",
                        "params": { "type": 3, "message": "fake server ready" },
                    }),
                );
                Some((
                    id,
                    json!({
                        "capabilities": {
                            "hoverProvider": true,
                            "completionProvider": { "resolveProvider": true },
                            "documentSymbolProvider": true,
                            "definitionProvider": true,
                            "referencesProvider": true,
                            "renameProvider": true,
                            "documentFormattingProvider": true,
                            "codeActionProvider": true,
                            "signatureHelpProvider": {},
                        }
                    }),
                ))
            }
            ("shutdown", Some(id)) => Some((id, Value::Null)),
            ("textDocument/hover", Some(id)) => Some((
                id,
                json!({ "contents": { "kind": "markdown", "value": "fake hover" } }),
            )),
            ("textDocument/completion", Some(id)) => Some((
                id,
                json!({ "isIncomplete": false, "items": [{ "label": "fake_item", "kind": 6 }] }),
            )),
            ("completionItem/resolve", Some(id)) => {
                let mut item = message["params"].clone();
                item["detail"] = json!("resolved by the fake server");
                Some((id, item))
            }
            ("textDocument/documentSymbol", Some(id)) => Some((id, json!([]))),
            ("textDocument/definition", Some(id)) => Some((id, Value::Null)),
            ("textDocument/references", Some(id)) => Some((id, json!([]))),
            ("textDocument/rename", Some(id)) => Some((id, json!({ "changes": {} }))),
            ("textDocument/prepareRename", Some(id)) => Some((id, Value::Null)),
            ("textDocument/formatting", Some(id)) => Some((id, json!([]))),
            ("textDocument/codeAction", Some(id)) => Some((id, json!([]))),
            ("textDocument/signatureHelp", Some(id)) => Some((id, Value::Null)),
            ("workspace/symbol", Some(id)) => Some((id, json!([]))),
            (_, Some(id)) => Some((
                id,
                json!({ "error": { "code": -32601, "message": "method not found" } }),
            )),
            (_, None) => None,
        };

        if let Some((id, payload)) = response {
            let mut message = json!({ "jsonrpc": "2.0", "id": id });
            if let Some(object) = message.as_object_mut() {
                match payload {
                    Value::Object(fields) if fields.contains_key("error") => {
                        object.insert("error".to_string(), fields["error"].clone());
                    }
                    other => {
                        object.insert("result".to_string(), other);
                    }
                }
            }
            let _ = send(&mut out, message);
        }

        if method == "textDocument/didOpen" {
            let uri = message["params"]["textDocument"]["uri"]
                .as_str()
                .unwrap_or_default();
            let _ = send(
                &mut out,
                json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": uri,
                        "diagnostics": [{
                            "range": {
                                "start": { "line": 0, "character": 1 },
                                "end": { "line": 0, "character": 4 },
                            },
                            "severity": 2,
                            "message": "fake warning",
                            "source": "fake",
                            "code": "F001",
                        }],
                    },
                }),
            );
        }
    }
}

fn send(out: &mut impl Write, message: Value) -> io::Result<()> {
    let mut frame = Vec::new();
    fergents_lsp::write_message(&mut frame, &message).map_err(io::Error::other)?;
    out.write_all(&frame)?;
    out.flush()
}
