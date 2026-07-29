//! `paredit lsp` — the Language Server Protocol over stdio.
//!
//! Every analysis this exposes already existed as a command. What the protocol
//! adds is *incrementality*: an editor asks about the buffer as it is being
//! typed, not about a file on disk, and it asks tens of times a second. A
//! process launch per keystroke is not that, which is why this is a server and
//! not a shell script around the CLI.
//!
//! Where the mapping is not obvious, the reason is in `features.rs`. The one
//! request worth naming here is `textDocument/selectionRange`: expanding a
//! selection outward through balanced expressions is the primary way a person
//! navigates Lisp, it has no CLI equivalent, and it is the thing this server
//! does that a general-purpose one cannot.
//!
//! This lives in the composition root rather than in a package because
//! diagnostics run the lint registry, which enumerates every rule and therefore
//! cannot live in core or in a feature (section 11.5.1).

mod documents;
mod features;
mod server;

use std::process::ExitCode;

use clap::Args;

use paredit_core_jsonrpc::{Framing, serve_stdio};

#[derive(Debug, Args)]
pub(crate) struct LspArgs {}

/// Runs the server until the client closes stdin or sends `exit`.
///
/// Returns an exit code rather than a `Result` because a broken pipe is how a
/// language server session normally ends — the editor was closed — and printing
/// `Error: broken pipe` to a stderr nobody is reading, with a failing status,
/// would make every clean shutdown look like a crash.
pub(crate) fn lsp(_args: LspArgs) -> ExitCode {
    let mut server = server::Server::default();
    match serve_stdio(&mut server, Framing::Header) {
        Ok(code) => ExitCode::from(code),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: paredit lsp: {error}");
            ExitCode::FAILURE
        }
    }
}
