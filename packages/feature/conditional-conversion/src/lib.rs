#![doc = include_str!("../README.md")]

pub mod conditional_conversion;
pub mod conditional_sugar;
pub mod convert_cond_to_if;
pub mod convert_if_to_cond;
pub mod convert_if_to_unless;
pub mod convert_if_to_when;
pub mod convert_unless_to_if;
pub mod convert_when_to_if;
pub mod error;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use convert_cond_to_if::cli::{ConvertCondToIfArgs, convert_cond_to_if};
pub use convert_if_to_cond::cli::{ConvertIfToCondArgs, convert_if_to_cond};

pub use error::{ConditionalConversionError, ConditionalConversionResult, ConditionalShapeError};
