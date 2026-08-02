//! The `docstring-summary-line-too-long` lint rule: its detection and its
//! adapter.
//!
//! One rule, one directory. `rule` is what the registry registers; `domain` is
//! the detection it drives.
//!
//! There is no `usecase`/`cli` pair here, unlike
//! `paredit-feature-lint-control-flow`'s `redundant_progn`. Those two modules
//! exist to back a standalone `inspect <rule>` command, whose wiring lives in
//! `src/presentation` — outside this package and outside this change. The two
//! closest existing rules, `paredit-feature-lint-convention`'s
//! `missing_docstring` and `commented_out_code`, are shaped exactly this way
//! for the same reason.

pub mod domain;
pub mod rule;
