//! Why a generator declined to write what it produced.
//!
//! Section 9.2. Six generators guard their own output the same way: they build
//! new source, splice it in, re-parse the result, and refuse if it does not
//! read back. That guard was written as
//! `.context("the generated <thing> would leave the file unparseable")`, which
//! reached the boundary as `internal.unclassified` — the tool declining to
//! corrupt a file, reported as the tool being broken.
//!
//! It earns the same code as the CLI's own write guard,
//! `refusal.rewrite-does-not-reparse`, because a caller's response is
//! identical: do not retry, report it. Everything else these commands refuse
//! is stated at the call site with `FeatureRefusal::message`, since those
//! refusals carry no inner error worth keeping.

use thiserror::Error;

use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_syntax::sexpr::ParseError;

/// A generator produced source that would not read back.
///
/// `summary` is the sentence each generator already used, so the rendering is
/// unchanged; the [`ParseError`] stays reachable as the source rather than
/// being flattened into it, which is what `.context()` did.
#[derive(Debug, Error)]
#[error("{summary}")]
pub struct GeneratedOutputWouldNotParse {
    pub summary: &'static str,
    #[source]
    pub source: ParseError,
}

paredit_core_cli::impl_classified_refusal!(GeneratedOutputWouldNotParse, |_error| {
    ErrorCode::RefusalRewriteDoesNotReparse
});
