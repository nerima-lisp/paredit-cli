//! Typed policy-gate failure. Gates requested via `--fail-on-*` /
//! `--require-*` flags exit with a dedicated status code so automation can
//! distinguish "gate tripped as designed" (exit 3) from hard errors (exit 1)
//! and usage errors (exit 2).
//!
//! A gate failure is not a malfunction. The command read its inputs, produced
//! its report, wrote it, and *then* found that the report violates a threshold
//! the caller asked to be held to. It travels in the `Err` channel only
//! because `--fail-on-*` is a request for a non-zero exit, and `main` gets its
//! exit status from a `Result`.
//!
//! It used to travel there type-erased — `gate_failure` built an
//! `anyhow::Error` and [`crate::diagnosis`] recovered the marker with
//! `downcast_ref`. It is now a variant of [`crate::CommandFailure`], which is
//! what removed `anyhow` from every command entry point in the workspace.

use thiserror::Error;

/// Exit status used when a requested policy gate fails after the report was
/// printed.
pub const GATE_FAILURE_EXIT_CODE: i32 = 3;

/// A requested `--fail-on-*` / `--require-*` gate tripped.
///
/// The message is built by the command, because only the command knows which
/// threshold it was measuring and by how much the report missed it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct GateFailure(pub String);

/// Builds a gate failure, ready to return from a command entry point.
///
/// Returns `CommandFailure` rather than [`GateFailure`] so that the ~30
/// existing `return Err(gate_failure(format!(...)))` sites keep compiling
/// unchanged: `Err(...)` performs no conversion of its own, only `?` does. The
/// bare variant is still reachable as `CommandFailure::Gate` for a caller that
/// wants to match on it.
pub fn gate_failure(message: impl Into<String>) -> crate::CommandFailure {
    crate::CommandFailure::Gate(GateFailure(message.into()))
}
