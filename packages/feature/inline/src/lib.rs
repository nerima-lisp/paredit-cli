#![doc = include_str!("../README.md")]

pub mod inline_function;
pub mod inline_lambda;
pub mod inline_let;
pub mod inline_local_function;
pub mod inline_symbol_macro;

// The contract with the composition root (section 4.2): each slice's `clap`
// argument type and the function that runs it.
pub use inline_function::cli::{InlineFunctionArgs, inline_function};
pub use inline_lambda::cli::{InlineLambdaArgs, inline_lambda};
pub use inline_let::cli::{InlineLetArgs, inline_let};
pub use inline_local_function::cli::{InlineLocalFunctionArgs, inline_local_function};
pub use inline_symbol_macro::cli::{InlineSymbolMacroArgs, inline_symbol_macro};
