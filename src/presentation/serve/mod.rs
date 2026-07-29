//! `paredit serve` — a resident analysis server over HTTP and JSON-RPC.
//!
//! A CLI invocation reads and parses a file every time it runs. Across a
//! repository and a loop, that repeated parse is the dominant cost, and it is
//! entirely avoidable: the file did not change between the outline call and the
//! lint call. This keeps one process alive with the answers in it.
//!
//! ## The two ways this could be a security bug, and what stops each
//!
//! A long-lived HTTP server on a developer's machine is a genuinely dangerous
//! shape, so both are handled rather than documented.
//!
//! **Anything local can talk to it.** Every process on the machine can reach a
//! loopback port, and this server reads files. So every request must carry a
//! bearer token minted at startup and printed to stderr; without it the server
//! answers 401 and reads nothing.
//!
//! **A web page can talk to it.** A page in the user's browser can POST to
//! `127.0.0.1` across origins, and with DNS rebinding it can read the reply.
//! The bearer *header* is what stops it: a cross-origin request cannot set one
//! without a CORS preflight, and this server approves none. That is also why
//! the token is not accepted in the URL or a cookie — both travel on a request
//! the browser makes by itself.
//!
//! Binding off loopback needs `--allow-remote`, which the argument's own help
//! text says sends the token over the network in clear text.

mod http;
mod service;

use std::io::BufReader;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use clap::Args;
use serde_json::{Value, json};

use paredit_core_jsonrpc::message::{Outcome, Request, response};
use paredit_core_jsonrpc::{ResponseError, error_codes};

#[derive(Debug, Args)]
pub(crate) struct ServeArgs {
    /// Address to listen on. Port 0 asks the operating system for a free one,
    /// which is then printed.
    #[arg(long, default_value = "127.0.0.1:0")]
    addr: String,
    /// Allow binding to an address other than loopback.
    ///
    /// The session token then travels over the network in clear text, and every
    /// host that can reach the port can read any file this process can. Use an
    /// SSH tunnel instead unless you have a specific reason.
    #[arg(long)]
    allow_remote: bool,
    /// Use this bearer token instead of a freshly minted one.
    ///
    /// For a supervisor that must know the token before the process starts. A
    /// token passed on a command line is visible in the process table.
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,
    /// Serve this many requests and exit. For scripts and tests that want a
    /// server for a bounded amount of work rather than a resident one.
    #[arg(long, value_name = "N")]
    max_requests: Option<u64>,
}

pub(crate) fn serve(args: ServeArgs) -> ExitCode {
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Error: paredit serve: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: ServeArgs) -> Result<(), String> {
    let listener = TcpListener::bind(&args.addr)
        .map_err(|error| format!("cannot bind {}: {error}", args.addr))?;
    let local = listener
        .local_addr()
        .map_err(|error| format!("cannot read the bound address: {error}"))?;

    if !is_loopback(local.ip()) && !args.allow_remote {
        return Err(format!(
            "refusing to listen on {local}: it is not a loopback address, and every host that \
             can reach the port could read any file this process can. Pass --allow-remote if \
             that is what you intend."
        ));
    }

    let token = match args.token {
        Some(token) if token.is_empty() => {
            return Err("--token must not be empty; an empty token authorizes everyone".to_owned());
        }
        Some(token) => token,
        None => mint_token(&local.to_string()),
    };

    // The address and token go to stderr, so a caller can capture the JSON that
    // stdout will carry without the banner in it.
    eprintln!("paredit serve listening on http://{local}");
    eprintln!("token: {token}");

    let cache = Arc::new(Mutex::new(service::Cache::default()));
    let mut served = 0_u64;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // Serialized rather than one thread per connection. The cache is the
        // shared state and every request touches it, so a thread per connection
        // would spend its life waiting on the same mutex — with the added
        // failure mode that a client holding a socket open blocks nothing today
        // and would block a worker then.
        handle(&stream, &cache, &token);
        served += 1;
        if args.max_requests.is_some_and(|limit| served >= limit) {
            return Ok(());
        }
    }
    Ok(())
}

const fn is_loopback(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_loopback(),
        IpAddr::V6(address) => address.is_loopback(),
    }
}

/// A session token.
///
/// Read from the operating system's random device, because the alternatives
/// are not good enough for something that authorizes reading files. A token
/// derived from the clock and the process id is guessable by anyone who knows
/// roughly when the server started — a search of about a billion, which is
/// minutes of local brute force — and a heap address is not random enough
/// either: two allocations in one process routinely land on the same address.
///
/// The clock-and-pid derivation survives only as the fallback for a platform
/// with no random device, where it is still better than a constant, and where
/// the header requirement is doing the load-bearing work against a browser
/// regardless.
fn mint_token(address: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    // `read_exact`, never `fs::read`. `/dev/urandom` is a character device with
    // no end, so a read-to-EOF never returns — it fills memory until the
    // process dies.
    let mut bytes = [0_u8; 32];
    let random = std::fs::File::open("/dev/urandom")
        .and_then(|mut device| std::io::Read::read_exact(&mut device, &mut bytes));
    match random {
        Ok(()) => hasher.update(&bytes),
        Err(_) => {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos());
            hasher.update(&nanos.to_le_bytes())
        }
    };
    hasher.update(address.as_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.finalize().to_hex()[..32].to_owned()
}

fn handle(stream: &TcpStream, cache: &Arc<Mutex<service::Cache>>, token: &str) {
    let mut reader = BufReader::new(stream);
    let request = match http::Request::read(&mut reader) {
        Ok(request) => request,
        Err(http::RequestError::Closed) => return,
        Err(error) => {
            let _ = write(stream, 400, "Bad Request", &format!("{error}\n"));
            return;
        }
    };

    // Unauthenticated, and deliberately before the route table: a health probe
    // that answered without a token would be a way to confirm the port is a
    // paredit server, which is the first step of an attack on it.
    if request.method == "GET" && request.target == "/health" && request.authorized(token) {
        let _ = write_json(stream, 200, "OK", &json!({ "status": "ok" }));
        return;
    }

    if !request.authorized(token) {
        let _ = write_json(
            stream,
            401,
            "Unauthorized",
            &json!({ "error": "an Authorization: Bearer <token> header is required" }),
        );
        return;
    }

    if request.method != "POST" {
        let _ = write_json(
            stream,
            405,
            "Method Not Allowed",
            &json!({ "error": "JSON-RPC calls are POSTed" }),
        );
        return;
    }

    let Ok(message) = serde_json::from_str::<Value>(&request.body) else {
        let _ = write_json(
            stream,
            200,
            "OK",
            &response(
                Value::Null,
                Err(ResponseError::new(
                    error_codes::PARSE_ERROR,
                    "request body is not JSON",
                )),
            ),
        );
        return;
    };

    let Some(call) = Request::from_value(&message) else {
        let _ = write_json(
            stream,
            200,
            "OK",
            &response(
                Value::Null,
                Err(ResponseError::new(
                    error_codes::INVALID_REQUEST,
                    "not a JSON-RPC request",
                )),
            ),
        );
        return;
    };

    let outcome = {
        // A poisoned mutex means a previous request panicked mid-analysis. The
        // cache is a pure derivation of files on disk, so the worst it can hold
        // is a stale entry; discarding the poison and continuing is better than
        // taking the server down for the rest of the session.
        let mut cache = cache.lock().unwrap_or_else(|poison| {
            let mut cache = poison.into_inner();
            *cache = service::Cache::default();
            cache
        });
        service::dispatch(&mut cache, &call.method, &call.params)
    };

    let Some(id) = call.id else {
        // A notification. HTTP still needs a response, so it is an empty one.
        let _ = write(stream, 204, "No Content", "");
        return;
    };
    let body = match outcome {
        Outcome::Reply(result) => response(id, Ok(result)),
        Outcome::Fail(error) => response(id, Err(error)),
        Outcome::Silent | Outcome::Stop(_) => response(id, Ok(Value::Null)),
    };
    let _ = write_json(stream, 200, "OK", &body);
}

fn write_json(stream: &TcpStream, status: u16, reason: &str, body: &Value) -> std::io::Result<()> {
    let text = serde_json::to_string(body)?;
    write_with(stream, status, reason, "application/json", &text)
}

fn write(stream: &TcpStream, status: u16, reason: &str, body: &str) -> std::io::Result<()> {
    write_with(stream, status, reason, "text/plain; charset=utf-8", body)
}

fn write_with(
    stream: &TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let mut stream = stream.try_clone()?;
    http::respond(&mut stream, status, reason, content_type, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A token that repeats is one an attacker can predict from a token they
    /// have already seen — or hardcode outright.
    #[test]
    fn a_minted_token_is_hex_and_never_repeats() {
        let tokens: Vec<String> = (0..8).map(|_| mint_token("127.0.0.1:9000")).collect();
        for token in &tokens {
            assert_eq!(token.len(), 32);
            assert!(token.chars().all(|c| c.is_ascii_hexdigit()), "{token}");
        }
        let mut unique = tokens.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            tokens.len(),
            "the same address and process id must still mint distinct tokens"
        );
    }

    #[test]
    fn loopback_is_recognised_for_both_address_families() {
        assert!(is_loopback("127.0.0.1".parse().expect("v4")));
        assert!(is_loopback("::1".parse().expect("v6")));
        assert!(!is_loopback("0.0.0.0".parse().expect("v4")));
        assert!(!is_loopback("192.168.1.10".parse().expect("v4")));
    }
}
