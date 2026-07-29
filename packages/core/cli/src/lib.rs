#![doc = include_str!("../README.md")]

pub mod args;
pub mod diagnosis;
pub mod error;
pub mod gate;
pub mod messages;
pub mod progress;
pub mod runtime;
// `io`, `diff` and `macos_acl` are `shared`'s submodules, declared there with
// `#[path]` so they can sit as sibling files. `macos_acl` is macOS-only.
pub mod shared;
pub mod workspace_args;

/// Renders a value through [`shared::terminal_safe`], which strips control
/// sequences so untrusted source text cannot drive a terminal.
///
/// Exported because every command's rendering path needs it. The root crate
/// keeps its own textually-scoped copy for the modules that have not moved
/// yet; both expand to the same function.
#[macro_export]
macro_rules! safe_text {
    ($value:expr) => {
        $crate::shared::terminal_safe(&$value)
    };
}

pub mod report;

pub use error::{ArgumentError, CleanupFailure, CliError, CliResult, IoRefusal, WriteTargetError};
