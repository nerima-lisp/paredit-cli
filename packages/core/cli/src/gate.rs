//! Typed policy-gate failure. Gates requested via `--fail-on-*` /
//! `--require-*` flags exit with a dedicated status code so automation can
//! distinguish "gate tripped as designed" (exit 3) from hard errors (exit 1)
//! and usage errors (exit 2).

use thiserror::Error;

/// Exit status used when a requested policy gate fails after the report was
/// printed.
pub(crate) const GATE_FAILURE_EXIT_CODE: i32 = 3;

#[derive(Debug, Error)]
#[error("{0}")]
pub(crate) struct GateFailure(pub(crate) String);

/// Builds an [`anyhow::Error`] carrying the gate marker type.
pub(crate) fn gate_failure(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(GateFailure(message.into()))
}
