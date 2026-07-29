//! Presentation adapters that map delivery mechanisms — the command line, and
//! the protocol servers — onto application services.

pub mod cli;
pub(crate) mod lsp;
pub(crate) mod mcp;
