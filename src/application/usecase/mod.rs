//! Application use cases for Lisp-aware analysis, reporting, and refactor planning.
//!
//! These services orchestrate typed domain operations into agent-facing plans,
//! reports, and workspace workflows without coupling to the CLI shell.

pub mod lint_report;
pub mod semantic_coverage;
