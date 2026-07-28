#![doc = include_str!("../README.md")]

pub mod error;
pub mod extract_constant;
pub mod extract_function;
pub mod extract_local_function;

// The contract with the composition root (section 4.2): each slice's `clap`
// argument type and the function that runs it.
pub use extract_constant::cli::{ExtractConstantArgs, extract_constant};
pub use extract_function::cli::{ExtractFunctionArgs, extract_function};
pub use extract_local_function::cli::{ExtractLocalFunctionArgs, extract_local_function};

pub use error::{ExtractionError, ExtractionResult, ExtractionScopeError, ExtractionTargetError};
