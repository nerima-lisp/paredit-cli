//! The names a rename cannot see, because they do not exist until macro time.
//!
//! Every rename in this tool is syntactic: it finds the atoms whose text is the
//! symbol and rewrites them. That is exactly right for code a reader can see,
//! and it is blind to a name that is *assembled*:
//!
//! ```lisp
//! (defmacro define-handler (name)
//!   `(defun ,(intern (format nil "HANDLE-~a" name)) (event) ...))
//!
//! (defun call-it (x)
//!   (funcall (intern "HANDLE-CLICK") x))
//! ```
//!
//! Renaming `handle-click` rewrites nothing here. The definition's name is
//! built from a format string and the call site's is a string literal, and a
//! rename that reports "2 occurrences renamed" while leaving both behind has
//! told the caller something false.
//!
//! This module does not attempt to *follow* the construction — that would mean
//! evaluating arbitrary Lisp at analysis time, which is the thing this tool
//! exists not to do. It reports the sites, so an incomplete rename is a
//! disclosed incompleteness rather than a silent one.
//!
//! Two kinds of site, because the caller's response differs:
//!
//! - **A literal that names the target.** `(intern "HANDLE-CLICK")` — the
//!   rename will certainly miss this, and the fix is a manual edit.
//! - **A computed name.** `(intern (format nil "HANDLE-~a" kind))` — the
//!   rename *may* miss it and nothing short of running the code can say. The
//!   fix is to look.

pub mod domain;

pub use domain::{
    ConstructionKind, MacroConstructionSite, SYMBOL_CONSTRUCTORS, find_macro_construction_sites,
};
