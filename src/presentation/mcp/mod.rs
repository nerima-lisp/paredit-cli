//! `paredit mcp` — the Model Context Protocol over stdio.
//!
//! The `skills/SKILL.md` shipped with this repository already tells an agent
//! that paredit exists and roughly what to run. This is the denser binding: the
//! agent discovers the tools from the server, gets a JSON Schema for each
//! argument, and gets results as structured content instead of scraped text.
//!
//! What it deliberately does *not* do is expose 314 tools. See `tools.rs`.

mod server;
mod tools;

use std::process::ExitCode;

use clap::Args;

use paredit_core_jsonrpc::{Framing, serve_stdio};

#[derive(Debug, Args)]
pub(crate) struct McpArgs {
    /// Refuse every command that would modify a file, whichever tool asks.
    ///
    /// For an agent pointed at a repository it should analyse and not edit. The
    /// refusal happens before the command runs, so it is a promise about the
    /// filesystem rather than a report about it.
    #[arg(long)]
    read_only: bool,
}

pub(crate) fn mcp(args: McpArgs) -> ExitCode {
    let mut server = server::Server {
        read_only: args.read_only,
    };
    // Line framing, not the language server's length-prefixed headers: MCP's
    // stdio transport specifies one JSON object per line.
    match serve_stdio(&mut server, Framing::Line) {
        Ok(code) => ExitCode::from(code),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: paredit mcp: {error}");
            ExitCode::FAILURE
        }
    }
}
