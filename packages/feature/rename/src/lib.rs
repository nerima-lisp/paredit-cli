#![doc = include_str!("../README.md")]

pub mod error;
pub mod rename;
pub mod rename_control;
pub mod rename_types;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use rename::cli::{RenameAtArgs, rename_at};
pub use rename::cli::{RenameBindingArgs, rename_binding};
pub use rename::cli::{RenameFunctionArgs, rename_function};
pub use rename::cli::{RenameInFormArgs, rename_in_form};
pub use rename::cli::{RenameLocalFunctionArgs, rename_local_function};
pub use rename::cli::{RenameMacroletArgs, rename_macrolet};
pub use rename::cli::{RenameSymbolArgs, rename_symbol};
pub use rename::cli::{RenameSymbolMacroArgs, rename_symbol_macro};
pub use rename::cli::{RenameSymbolsArgs, rename_symbols};
pub use rename::cli::{ReplaceFunctionCallsArgs, replace_function_calls};
pub use rename::cli::{UnwrapFunctionCallsArgs, unwrap_function_calls};
pub use rename::cli::{WrapFunctionCallsArgs, wrap_function_calls};
pub use rename_control::cli::{RenameBlockArgs, rename_block};
pub use rename_control::cli::{RenameTagArgs, rename_tag};

pub use error::{
    BindingListError, BindingSelectionError, CallSiteError, RenameControlError, RenameError,
    RenameResult, SemanticShapeError,
};
