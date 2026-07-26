//! Common Lisp `prog2`-to-`progn` detection: a two-argument `(prog2 a b)`.
//!
//! `prog2` evaluates its forms left to right and returns the value(s) of the
//! *second*; with exactly two forms the second is also the last, so
//! `(prog2 a b)` returns the same value(s) as `(progn a b)` — same evaluation
//! order, same result. `progn` states the sequencing without the (now
//! irrelevant) "return the second form" twist of `prog2`.
//!
//! Only the exact two-form shape is matched. A `(prog2 a b c …)` returns `b`
//! (the second form), which `progn` (returning the last) cannot express, so it
//! is left alone, as is a one-form `(prog2 a)` and a reader-conditional body.
//!
//! The fix rewrites the operator token `prog2` to `progn`, leaving the two forms
//! byte-identical, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct Prog2ToPrognItem {
    pub path: PathBuf,
    /// The span of the whole `(prog2 a b)` form.
    pub span: ByteSpan,
    /// The span of the `prog2` operator token (rewritten to `progn`).
    pub head_span: ByteSpan,
}

#[derive(Debug)]
pub struct Prog2ToPrognSummary {
    pub prog2_form_count: usize,
    pub violations: Vec<Prog2ToPrognItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct Prog2ToPrognPolicyOptions {
    fail_on_violation: bool,
}

impl Prog2ToPrognPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct Prog2ToPrognPolicy {
    pub fail_on_violation: bool,
    pub prog2_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    prog2_form_count: &mut usize,
    violations: &mut Vec<Prog2ToPrognItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("prog2") {
        return;
    }
    *prog2_form_count += 1;

    // children: [prog2, a, b] — require exactly two body forms.
    if view.children.len() != 3 {
        return;
    }
    if view.children[1..].iter().any(is_reader_conditional) {
        return;
    }

    violations.push(Prog2ToPrognItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
    });
}

/// Collects every two-form `(prog2 a b)` across a whole file, along with the
/// total number of `prog2` forms scanned.
pub fn collect_prog2_to_progn(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<Prog2ToPrognItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut prog2_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut prog2_form_count, &mut violations);
        });
    }
    Ok((prog2_form_count, violations))
}

pub fn summarize_prog2_to_progn(
    prog2_form_count: usize,
    violations: Vec<Prog2ToPrognItem>,
) -> Prog2ToPrognSummary {
    Prog2ToPrognSummary {
        prog2_form_count,
        violations,
    }
}

pub fn evaluate_prog2_to_progn_policy(
    options: Prog2ToPrognPolicyOptions,
    summary: &Prog2ToPrognSummary,
) -> Prog2ToPrognPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    Prog2ToPrognPolicy {
        fail_on_violation: options.fail_on_violation(),
        prog2_form_count: summary.prog2_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prog2s(input: &str) -> (usize, Vec<Prog2ToPrognItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_prog2_to_progn(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect prog2 to progn")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_form_prog2() {
        let source = "(prog2 (setup) (run))";
        let (count, violations) = prog2s(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].head_span), "prog2");
    }

    #[test]
    fn does_not_flag_three_form_prog2() {
        // (prog2 a b c) returns b, which progn cannot express.
        assert!(prog2s("(prog2 a b c)").1.is_empty());
    }

    #[test]
    fn does_not_flag_one_form_prog2() {
        assert!(prog2s("(prog2 a)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = prog2s("(PROG2 a b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = prog2s("(defun f (a b) (prog2 a b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(prog2 a b)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_prog2_to_progn(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect prog2 to progn");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = prog2s("(prog2 a b)");
        let summary = summarize_prog2_to_progn(count, items);

        let quiet = evaluate_prog2_to_progn_policy(Prog2ToPrognPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_prog2_to_progn_policy(Prog2ToPrognPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
