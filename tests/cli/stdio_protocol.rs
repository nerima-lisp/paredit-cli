//! Stdio protocol adapters, driven as real child processes.
//!
//! The shared JSON-RPC crate exercises both framings over in-memory streams.
//! These checks keep the CLI adapters from accidentally selecting the wrong
//! framing when they are wired to the process's actual stdin and stdout.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

fn run_stdio(arguments: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_paredit"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn paredit protocol server");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin.write_all(input).expect("write protocol request");
    drop(stdin);
    child.wait_with_output().expect("wait for protocol server")
}

fn lsp_message(value: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&value).expect("serialize LSP request");
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

fn lsp_response(output: &[u8]) -> Value {
    let response = std::str::from_utf8(output).expect("LSP response is UTF-8");
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .expect("LSP response has a header delimiter");
    let declared: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .expect("LSP response has Content-Length")
        .parse()
        .expect("Content-Length is numeric");
    assert_eq!(declared, body.len(), "Content-Length counts response bytes");
    serde_json::from_str(body).expect("LSP response body is JSON")
}

#[test]
fn cli_lsp_uses_content_length_framing_over_stdio() {
    let mut input = lsp_message(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": { "general": { "positionEncodings": ["utf-8"] } } },
    }));
    input.extend(lsp_message(json!({ "jsonrpc": "2.0", "method": "exit" })));

    let output = run_stdio(&["lsp"], &input);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = lsp_response(&output.stdout);
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["capabilities"]["positionEncoding"],
        "utf-8"
    );
}

#[test]
fn cli_mcp_uses_newline_delimited_json_over_stdio() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"contract-test","version":"1"}}}"#,
        "\n",
    );

    let output = run_stdio(&["mcp"], input.as_bytes());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = std::str::from_utf8(&output.stdout).expect("MCP response is UTF-8");
    assert_eq!(
        response.lines().count(),
        1,
        "MCP writes one JSON response line"
    );
    assert!(response.ends_with('\n'), "MCP response ends in a newline");
    let value: Value = serde_json::from_str(response.trim_end()).expect("MCP response is JSON");
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 7);
    assert_eq!(value["result"]["protocolVersion"], "2024-11-05");
    assert!(
        value["result"]["capabilities"]["tools"].is_object(),
        "{value}"
    );
}
