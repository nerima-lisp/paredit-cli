//! The per-file Emacs Lisp facts an agent needs before editing a `.el` file.

pub use super::domain::{
    AutoloadEntry, EmacsLispFileFacts, EmacsLispFilePolicy, EmacsLispFilePolicyOptions,
    FeatureReference, LexicalBindingStatus, collect_emacs_lisp_file_facts,
    evaluate_emacs_lisp_file_policy, supports_emacs_lisp_file_report_dialect,
};
