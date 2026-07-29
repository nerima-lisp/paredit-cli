//! `paredit mcp`, driven over a real pipe.
//!
//! The handler is unit-tested next to itself. What only an end-to-end run
//! establishes is that a tool call actually reaches a command and comes back
//! with its output — the server executes commands as subprocesses of itself,
//! and nothing short of running one proves that path works.

use super::*;

fn line_framed(messages: &[serde_json::Value]) -> Vec<u8> {
    let mut payload = String::new();
    for message in messages {
        payload.push_str(&serde_json::to_string(message).expect("serialize"));
        payload.push('\n');
    }
    payload.into_bytes()
}

fn session(extra_args: &[&str], messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut command = paredit();
    command.arg("mcp").args(extra_args);
    let assert = command
        .write_stdin(line_framed(messages))
        .assert()
        .success();
    String::from_utf8(assert.get_output().stdout.clone())
        .expect("utf-8 stdout")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

fn initialize() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} },
    })
}

fn call(id: i64, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    })
}

fn reply(messages: &[serde_json::Value], id: i64) -> serde_json::Value {
    messages
        .iter()
        .find(|message| message["id"] == id)
        .unwrap_or_else(|| panic!("no reply with id {id} in {messages:#?}"))["result"]
        .clone()
}

fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, source).expect("write lisp fixture");
    file
}

/// The framing is the whole wire-level difference from the language server:
/// one compact JSON object per line, never a raw newline inside one.
#[test]
fn cli_mcp_writes_exactly_one_json_object_per_line() {
    let responses = session(&[], &[initialize()]);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "paredit");
}

#[test]
fn cli_mcp_lists_tools_with_schemas_a_client_can_validate_against() {
    let responses = session(
        &[],
        &[
            initialize(),
            serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        ],
    );
    let tools = reply(&responses, 2)["tools"]
        .as_array()
        .expect("tools")
        .clone();
    assert!(!tools.is_empty());
    for tool in &tools {
        assert!(tool["name"].as_str().is_some_and(|name| !name.is_empty()));
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
    }
}

/// The path that only an end-to-end run covers: a tool call re-executes this
/// binary and brings its stdout back as content.
#[test]
fn cli_mcp_runs_a_command_and_returns_its_output() {
    let file = fixture("mcp-lint", "(defun f (x) (if (not x) 1 2))\n");
    let responses = session(
        &[],
        &[
            initialize(),
            call(
                2,
                "paredit_lint",
                serde_json::json!({ "paths": [file.display().to_string()] }),
            ),
        ],
    );

    let result = reply(&responses, 2);
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().expect("text content");
    assert!(text.contains("negated-if"), "{text}");
    // The payload is the command's own JSON, unchanged.
    let report: serde_json::Value = serde_json::from_str(text).expect("the command's JSON");
    assert!(
        report["finding_count"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
}

#[test]
fn cli_mcp_reads_the_capability_catalog_as_a_resource() {
    let responses = session(
        &[],
        &[
            initialize(),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "resources/read",
                "params": { "uri": "paredit://capabilities" },
            }),
        ],
    );
    let contents = reply(&responses, 2)["contents"][0].clone();
    assert_eq!(contents["mimeType"], "application/json");
    let catalog: serde_json::Value =
        serde_json::from_str(contents["text"].as_str().expect("text")).expect("catalog parses");
    assert_eq!(catalog["schema_version"], 3);
    assert!(catalog["dialect_contract"].is_object(), "{catalog}");
}

/// `--read-only` is a promise about the filesystem, so the test that matters is
/// that the file is untouched — not that the response says "refused".
#[test]
fn cli_mcp_read_only_leaves_the_file_exactly_as_it_found_it() {
    let source = "(defun f (x)\n(if (not x) 1 2))\n";
    let file = fixture("mcp-read-only", source);
    let responses = session(
        &["--read-only"],
        &[
            initialize(),
            call(
                2,
                "paredit_run",
                serde_json::json!({
                    "args": ["edit", "format", "--file", file.display().to_string(), "--write"],
                }),
            ),
        ],
    );

    assert_eq!(reply(&responses, 2)["isError"], true);
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        source,
        "--read-only must not merely report; the file must be untouched"
    );
}

/// A command that spells the write in its *name* rather than in a flag.
///
/// `--read-only` was a flag list — `--write`, `--fix`, `--apply`,
/// `--in-place` — and that was a complete gate for exactly as long as every
/// writing command carried one of them. `paredit fix apply` carries none: the
/// whole point of the `fix` namespace is that the write is in the command
/// name. It ran, and it wrote, while the server's own `initialize` response
/// said "every command that would modify a file is refused".
#[test]
fn cli_mcp_read_only_refuses_a_command_that_writes_without_a_writing_flag() {
    let source = "(defun f (x)\n  (setf x (1+ x))\n  x)\n";
    let file = fixture("mcp-read-only-fix", source);
    let responses = session(
        &["--read-only"],
        &[
            initialize(),
            call(
                2,
                "paredit_run",
                serde_json::json!({
                    "args": ["fix", "apply", file.display().to_string()],
                }),
            ),
        ],
    );

    assert_eq!(reply(&responses, 2)["isError"], true);
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        source,
        "--read-only must refuse `fix apply`, which writes with no writing flag"
    );
}

/// The gate matches a leading subcommand *sequence*, not any occurrence of the
/// word, or it would refuse reads that merely mention it.
#[test]
fn cli_mcp_read_only_still_permits_a_read_that_merely_mentions_a_writing_command() {
    let responses = session(
        &["--read-only"],
        &[
            initialize(),
            call(
                2,
                "paredit_run",
                serde_json::json!({ "args": ["fix", "list"] }),
            ),
        ],
    );
    assert_eq!(reply(&responses, 2)["isError"], false);
}

/// Without `--read-only` the same call is allowed, or the flag would be
/// meaningless.
#[test]
fn cli_mcp_without_read_only_permits_a_writing_command() {
    let file = fixture("mcp-writable", "(defun f (x)\n(if x 1 2))\n");
    let responses = session(
        &[],
        &[
            initialize(),
            call(
                2,
                "paredit_run",
                serde_json::json!({
                    "args": ["edit", "format", "--file", file.display().to_string(), "--write"],
                }),
            ),
        ],
    );

    assert_eq!(reply(&responses, 2)["isError"], false);
    let written = fs::read_to_string(&file).expect("read fixture");
    assert!(
        written.starts_with("(defun f (x)\n  (if x"),
        "the body should have been indented under the defun: {written}"
    );
}

/// Exit code 3 is a gate reporting what it found. A model told that was an
/// error retries a command that worked.
#[test]
fn cli_mcp_reports_a_failed_gate_as_a_result_rather_than_a_tool_error() {
    let file = fixture("mcp-gate", "(defun f (x) (if (not x) 1 2))\n");
    let responses = session(
        &[],
        &[
            initialize(),
            call(
                2,
                "paredit_run",
                serde_json::json!({
                    "args": [
                        "inspect", "lint", "--fail-on-finding",
                        file.display().to_string(),
                    ],
                }),
            ),
        ],
    );

    let result = reply(&responses, 2);
    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["gate_failed"], true);
    assert_eq!(result["structuredContent"]["exit_code"], 3);
}
