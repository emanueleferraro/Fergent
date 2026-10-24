//! `Content-Length` framing over a byte stream (LSP base protocol).

use std::io::{BufRead, Write};

use serde_json::Value;

use crate::error::LspError;

/// Write one JSON-RPC message with a `Content-Length` header.
pub fn write_message(out: &mut impl Write, message: &Value) -> Result<(), LspError> {
    let body = serde_json::to_vec(message)?;
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(&body)?;
    out.flush()?;
    Ok(())
}

/// Read one message, or `None` at end of stream.
///
/// Headers are read line by line (tolerating `\n` or `\r\n` and extra
/// headers); the body is read by byte length, so multi-byte characters are
/// sized correctly.
pub fn read_message(input: &mut impl BufRead) -> Result<Option<Value>, LspError> {
    let mut length: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = input.read_line(&mut line)?;
        if read == 0 {
            return if saw_header {
                Err(LspError::Protocol("truncated headers".to_string()))
            } else {
                Ok(None)
            };
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if saw_header {
                break;
            }
            continue;
        }
        saw_header = true;
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }

    let length = length.ok_or_else(|| LspError::Protocol("missing Content-Length".to_string()))?;
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_a_message() {
        let mut buffer = Vec::new();
        write_message(
            &mut buffer,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
        )
        .unwrap();
        let mut reader = std::io::BufReader::new(buffer.as_slice());
        let message = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(message["method"], json!("initialize"));
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn lengths_are_bytes_not_characters() {
        let message = json!({ "text": "héllo 🦀" });
        let mut buffer = Vec::new();
        write_message(&mut buffer, &message).unwrap();
        assert!(String::from_utf8_lossy(&buffer).contains(&format!(
            "Content-Length: {}",
            serde_json::to_vec(&message).unwrap().len()
        )));
        let mut reader = std::io::BufReader::new(buffer.as_slice());
        assert_eq!(read_message(&mut reader).unwrap().unwrap(), message);
    }

    #[test]
    fn tolerates_extra_headers_and_lf_only() {
        let body = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
        let frame = format!(
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\nContent-Length: {}\n\n{body}",
            body.len()
        );
        let mut reader = std::io::BufReader::new(frame.as_bytes());
        let message = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(message["method"], json!("initialized"));
    }

    #[test]
    fn rejects_a_missing_length() {
        let frame = b"Content-Type: application/json\r\n\r\n{}";
        let mut reader = std::io::BufReader::new(frame.as_slice());
        assert!(matches!(
            read_message(&mut reader),
            Err(LspError::Protocol(_))
        ));
    }

    #[test]
    fn rejects_a_truncated_body() {
        let frame = b"Content-Length: 100\r\n\r\n{}";
        let mut reader = std::io::BufReader::new(frame.as_slice());
        assert!(matches!(read_message(&mut reader), Err(LspError::Io(_))));
    }
}
