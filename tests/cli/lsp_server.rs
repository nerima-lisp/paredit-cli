//! `paredit lsp`, driven over a real pipe.
//!
//! The handler's own behaviour is unit-tested next to it. What only an
//! end-to-end run can establish is the part in between: that the binary speaks
//! the framing an editor speaks, that the session terminates, and that a
//! client dropping the pipe is a clean exit rather than a crash. Every one of
//! those is invisible to a test that calls the handler directly, and every one
//! of them presents to a user as "the extension does not work".

use super::*;

/// Wraps messages in the LSP's `Content-Length` framing.
fn framed(messages: &[serde_json::Value]) -> Vec<u8> {
    let mut payload = Vec::new();
    for message in messages {
        let body = serde_json::to_string(message).expect("serialize");
        payload.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        payload.extend_from_slice(body.as_bytes());
    }
    payload
}

/// Splits a framed response stream back into messages.
///
/// Parsing the framing rather than searching the text is the point: a server
/// whose declared length disagrees with its body would still contain the right
/// substrings, and an editor would still hang on it.
fn unframe(mut stream: &[u8]) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    while !stream.is_empty() {
        let text = std::str::from_utf8(stream).expect("utf-8 stream");
        let Some(separator) = text.find("\r\n\r\n") else {
            break;
        };
        let length: usize = text[..separator]
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .expect("a Content-Length header")
            .trim()
            .parse()
            .expect("a numeric length");
        let start = separator + 4;
        let body = &stream[start..start + length];
        messages.push(serde_json::from_slice(body).expect("a JSON body"));
        stream = &stream[start + length..];
    }
    messages
}

fn session(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let assert = paredit()
        .arg("lsp")
        .write_stdin(framed(messages))
        .assert()
        .success();
    unframe(&assert.get_output().stdout)
}

fn initialize() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {} },
    })
}

fn did_open(text: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///tmp/paredit-lsp/core.lisp",
                "languageId": "commonlisp",
                "version": 1,
                "text": text,
            },
        },
    })
}

fn exit() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "jsonrpc": "2.0", "id": 99, "method": "shutdown" }),
        serde_json::json!({ "jsonrpc": "2.0", "method": "exit" }),
    ]
}

fn reply(messages: &[serde_json::Value], id: i64) -> serde_json::Value {
    messages
        .iter()
        .find(|message| message["id"] == id)
        .unwrap_or_else(|| panic!("no reply with id {id} in {messages:#?}"))["result"]
        .clone()
}

#[test]
fn cli_lsp_completes_a_session_and_exits_zero() {
    let mut messages = vec![initialize()];
    messages.extend(exit());
    let responses = session(&messages);

    let capabilities = reply(&responses, 1)["capabilities"].clone();
    assert_eq!(capabilities["selectionRangeProvider"], true);
    assert_eq!(reply(&responses, 99), serde_json::Value::Null);
}

/// The framing is the contract with the editor. A declared length that
/// disagrees with the body desynchronises the stream, and the symptom is a hung
/// editor rather than an error.
#[test]
fn cli_lsp_frames_every_response_with_an_accurate_byte_length() {
    let mut messages = vec![initialize(), did_open("(f \"🎈\" x)\n")];
    messages.extend(exit());
    // `unframe` reads each declared length and would panic or mis-split if any
    // were wrong; reaching the end with well-formed JSON is the assertion.
    let responses = session(&messages);
    assert!(responses.len() >= 3, "{responses:#?}");
}

#[test]
fn cli_lsp_publishes_diagnostics_for_an_opened_document() {
    let mut messages = vec![initialize(), did_open("(defun f (x) (if (not x) 1 2))\n")];
    messages.extend(exit());
    let responses = session(&messages);

    let published = responses
        .iter()
        .find(|message| message["method"] == "textDocument/publishDiagnostics")
        .expect("a publishDiagnostics notification");
    assert!(
        published["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "negated-if"),
        "{published:#?}"
    );
}

/// The request this server answers better than a general-purpose one: expanding
/// a selection outward through balanced expressions.
#[test]
fn cli_lsp_answers_selection_range_with_the_enclosing_expressions() {
    let mut messages = vec![
        initialize(),
        did_open("(defun f (x)\n  (+ x 1))\n"),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/selectionRange",
            "params": {
                "textDocument": { "uri": "file:///tmp/paredit-lsp/core.lisp" },
                "positions": [{ "line": 1, "character": 5 }],
            },
        }),
    ];
    messages.extend(exit());
    let responses = session(&messages);

    let first = reply(&responses, 2)[0].clone();
    // `x`, then `(+ x 1)`, then the `defun`, then the document.
    assert_eq!(
        first["range"]["start"],
        serde_json::json!({ "line": 1, "character": 5 })
    );
    assert_eq!(
        first["parent"]["range"]["start"],
        serde_json::json!({ "line": 1, "character": 2 })
    );
    assert_eq!(
        first["parent"]["parent"]["range"]["start"],
        serde_json::json!({ "line": 0, "character": 0 })
    );
}

/// An editor closing is how a language server session normally ends. Reporting
/// it as a failure would make every clean shutdown look like a crash.
#[test]
fn cli_lsp_exits_zero_when_the_client_closes_the_stream_without_saying_goodbye() {
    paredit()
        .arg("lsp")
        .write_stdin(framed(&[initialize(), did_open("(f)\n")]))
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

/// A half-typed form is the normal state of a buffer. Reporting every rule
/// against a recovered tree is what makes people turn a language server off.
#[test]
fn cli_lsp_reports_only_the_parse_failure_for_an_unbalanced_buffer() {
    let mut messages = vec![initialize(), did_open("(defun f (x)\n  (if (not x) 1 2)\n")];
    messages.extend(exit());
    let responses = session(&messages);

    let diagnostics = responses
        .iter()
        .find(|message| message["method"] == "textDocument/publishDiagnostics")
        .expect("a publishDiagnostics notification")["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .clone();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0]["code"], "parse");
}

/// One malformed message must not take the session with it: the requests queued
/// behind it are still owed answers.
#[test]
fn cli_lsp_survives_a_malformed_message() {
    let mut payload = b"Content-Length: 12\r\n\r\nnot json !!!".to_vec();
    let mut rest = vec![initialize()];
    rest.extend(exit());
    payload.extend(framed(&rest));

    let assert = paredit().arg("lsp").write_stdin(payload).assert().success();
    let responses = unframe(&assert.get_output().stdout);
    assert!(
        responses
            .iter()
            .any(|message| message["error"]["code"] == -32700),
        "{responses:#?}"
    );
    assert!(
        responses.iter().any(|message| message["id"] == 1),
        "{responses:#?}"
    );
}
