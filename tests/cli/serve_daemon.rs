//! `paredit serve`, driven over a real socket.
//!
//! The client here is twenty lines of `TcpStream` rather than an HTTP library,
//! for the same reason the server is: this exchange needs a request line, three
//! headers, and a body, and a dependency for that would be a dependency to keep
//! current forever.
//!
//! What these cover that the unit tests cannot: that the process binds, prints
//! an address and a token a caller can actually use, refuses a request without
//! one, and shares its cache across separate connections — which is the entire
//! reason to run a server instead of the CLI.

use super::*;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command as Process, Stdio};

/// A running server, and how to reach it.
struct Server {
    process: Child,
    address: String,
    token: String,
}

impl Server {
    /// Starts a server bound to a free loopback port, reading the address and
    /// token it prints.
    fn start(max_requests: usize) -> Self {
        let mut process = Process::new(env!("CARGO_BIN_EXE_paredit"))
            .args([
                "serve",
                "--addr",
                "127.0.0.1:0",
                "--max-requests",
                &max_requests.to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn paredit serve");

        let mut stderr = BufReader::new(process.stderr.take().expect("stderr is piped"));
        let mut banner = String::new();
        stderr.read_line(&mut banner).expect("the address line");
        let address = banner
            .rsplit_once("http://")
            .expect("an http:// address in the banner")
            .1
            .trim()
            .to_owned();

        let mut token_line = String::new();
        stderr.read_line(&mut token_line).expect("the token line");
        let token = token_line
            .trim()
            .strip_prefix("token: ")
            .expect("a token line")
            .to_owned();

        Self {
            process,
            address,
            token,
        }
    }

    /// POSTs a JSON-RPC body, returning the status code and the response body.
    fn post(&self, body: &serde_json::Value, token: Option<&str>) -> (u16, String) {
        let payload = serde_json::to_string(body).expect("serialize");
        let mut request = format!(
            "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n",
            self.address,
        );
        if let Some(token) = token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str(&format!("Content-Length: {}\r\n\r\n", payload.len()));
        request.push_str(&payload);

        let mut stream = TcpStream::connect(&self.address).expect("connect");
        stream.write_all(request.as_bytes()).expect("write request");
        stream.flush().expect("flush");

        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        let status: u16 = response
            .split_whitespace()
            .nth(1)
            .expect("a status code")
            .parse()
            .expect("a numeric status");
        let body = response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_owned())
            .unwrap_or_default();
        (status, body)
    }

    fn json(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let (status, body) = self.post(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            }),
            Some(&self.token),
        );
        assert_eq!(status, 200, "{method} answered {status}: {body}");
        serde_json::from_str(&body).unwrap_or_else(|_| panic!("{method} answered non-JSON: {body}"))
    }

    fn finish(mut self) {
        let _ = self.process.wait();
    }
}

fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, source).expect("write lisp fixture");
    file
}

#[test]
fn cli_serve_answers_a_json_rpc_call_over_http() {
    let server = Server::start(1);
    let result = server.json("paredit/version", serde_json::json!({}));
    assert_eq!(result["result"]["name"], "paredit");
    server.finish();
}

/// The token is what stands between a local port that reads files and every
/// other process on the machine.
#[test]
fn cli_serve_refuses_a_request_without_the_token() {
    let server = Server::start(2);
    let (status, body) = server.post(
        &serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "paredit/version" }),
        None,
    );
    assert_eq!(status, 401, "{body}");

    let (status, _) = server.post(
        &serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "paredit/version" }),
        Some("not-the-token"),
    );
    assert_eq!(status, 401);
    server.finish();
}

/// The whole reason to run a server: the second question about an unchanged
/// file costs nothing, across separate connections.
#[test]
fn cli_serve_reuses_one_parse_across_connections() {
    let file = fixture("serve-cache", "(defun f (x) (if (not x) 1 2))\n");
    let path = serde_json::json!({ "path": file.display().to_string() });
    let server = Server::start(4);

    let analyzed = server.json("paredit/analyze", path.clone());
    assert_eq!(analyzed["result"]["parsed"], true);
    assert!(
        analyzed["result"]["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["rule"] == "negated-if"),
        "{analyzed}"
    );

    server.json("paredit/outline", path.clone());
    server.json("paredit/lint", path);

    let stats = server.json("paredit/cache", serde_json::json!({}));
    assert_eq!(stats["result"]["misses"], 1, "{stats}");
    assert_eq!(stats["result"]["hits"], 2, "{stats}");
    server.finish();
}

/// An edited file must be re-read. The stamp is mtime plus length, so the
/// fixture below changes the length, which is what a real edit does.
#[test]
fn cli_serve_re_reads_a_file_that_changed() {
    let file = fixture("serve-changed", "(defun f () 1)\n");
    let path = serde_json::json!({ "path": file.display().to_string() });
    let server = Server::start(3);

    let before = server.json("paredit/outline", path.clone());
    assert_eq!(before["result"].as_array().expect("outline").len(), 1);

    fs::write(&file, "(defun f () 1)\n(defun g () 2)\n").expect("rewrite fixture");
    let after = server.json("paredit/outline", path.clone());
    assert_eq!(after["result"].as_array().expect("outline").len(), 2);

    let stats = server.json("paredit/cache", serde_json::json!({}));
    assert_eq!(stats["result"]["misses"], 2, "{stats}");
    server.finish();
}

#[test]
fn cli_serve_reports_an_unparseable_file_as_a_result_rather_than_a_failure() {
    let file = fixture("serve-unparseable", "(defun f (x)\n");
    let server = Server::start(1);
    let result = server.json(
        "paredit/analyze",
        serde_json::json!({ "path": file.display().to_string() }),
    );
    assert_eq!(result["result"]["parsed"], false);
    assert!(result["result"]["error"].is_string(), "{result}");
    server.finish();
}

/// Binding off loopback would let every host that can reach the port read any
/// file this process can. It has to be asked for explicitly.
#[test]
fn cli_serve_refuses_a_non_loopback_bind_without_allow_remote() {
    paredit()
        .args(["serve", "--addr", "0.0.0.0:0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--allow-remote"));
}

#[test]
fn cli_serve_refuses_an_empty_token() {
    paredit()
        .args(["serve", "--addr", "127.0.0.1:0", "--token", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("authorizes everyone"));
}
