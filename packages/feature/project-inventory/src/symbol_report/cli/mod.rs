mod render;
pub mod types;

pub mod args;
pub mod workflow;

// Hoisted for the composition root (section 4.2): the argument types and run
// functions of the two subcommands this slice owns.
pub use args::{SymbolQueryArgs, SymbolReportArgs};
pub use workflow::{find_symbol, symbol_report};
