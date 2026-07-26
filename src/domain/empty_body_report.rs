//! Common Lisp empty-body detection: a `when`, `unless`, `dolist`, or `dotimes`
//! form that has its test (or iteration spec) but no body — `(when ready)`,
//! `(unless done)`, `(dolist (x items))`, `(dotimes (i n))`. The test or spec is
//! evaluated and then nothing happens, so the form is pointless: `(when x)` is
//! just `(progn x nil)`, and an empty `dolist`/`dotimes` iterates doing nothing.
//! This is almost always a forgotten body or an editing leftover.
//!
//! Only these four forms are covered, and only when the fixed prefix is present
//! (the test for `when`/`unless`, the `(var …)` spec for `dolist`/`dotimes`) but
//! no body form follows. A form with a body, or a malformed form missing its
//! prefix entirely, is left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, list_head};

/// The index at which a body-requiring form's body begins, or `None` if the
/// head is not one of the covered forms. All four take a single prefix element
/// (a test or a spec), so their body starts at index 2.
fn body_start(head: &str) -> Option<usize> {
    if head.eq_ignore_ascii_case("when")
        || head.eq_ignore_ascii_case("unless")
        || head.eq_ignore_ascii_case("dolist")
        || head.eq_ignore_ascii_case("dotimes")
    {
        Some(2)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct EmptyBodyItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The form head (`when`, `unless`, `dolist`, or `dotimes`).
    pub head: String,
}

#[derive(Debug)]
pub struct EmptyBodySummary {
    pub body_form_count: usize,
    pub violations: Vec<EmptyBodyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EmptyBodyPolicyOptions {
    fail_on_violation: bool,
}

impl EmptyBodyPolicyOptions {
    #[must_use]
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EmptyBodyPolicy {
    pub fail_on_violation: bool,
    pub body_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_form(
    view: &ExpressionView,
    path: &Path,
    body_form_count: &mut usize,
    violations: &mut Vec<EmptyBodyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(start) = body_start(head) else {
        return;
    };
    *body_form_count += 1;

    // The prefix element (test/spec) is present, but nothing follows it: the
    // children are exactly [head, prefix]. A shorter form is missing its prefix
    // (malformed, a different concern); a longer one has a body.
    if view.children.len() == start {
        violations.push(EmptyBodyItem {
            path: path.to_path_buf(),
            span: view.span,
            head: head.to_ascii_lowercase(),
        });
    }
}

/// Collects every empty-bodied `when`/`unless`/`dolist`/`dotimes` across a whole
/// file, along with the total number of such forms scanned.
pub fn collect_empty_bodies(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EmptyBodyItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut body_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, path, &mut body_form_count, &mut violations);
        });
    }
    Ok((body_form_count, violations))
}

#[must_use]
pub fn summarize_empty_bodies(
    body_form_count: usize,
    violations: Vec<EmptyBodyItem>,
) -> EmptyBodySummary {
    EmptyBodySummary {
        body_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_empty_body_policy(
    options: EmptyBodyPolicyOptions,
    summary: &EmptyBodySummary,
) -> EmptyBodyPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EmptyBodyPolicy {
        fail_on_violation: options.fail_on_violation(),
        body_form_count: summary.body_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies(input: &str) -> (usize, Vec<EmptyBodyItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_empty_bodies(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect empty bodies")
    }

    #[test]
    fn flags_when_with_no_body() {
        let (count, violations) = bodies("(when ready)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
    }

    #[test]
    fn flags_unless_with_no_body() {
        let (_, violations) = bodies("(unless done)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "unless");
    }

    #[test]
    fn flags_dolist_and_dotimes_with_no_body() {
        let (_, violations) = bodies("(dolist (x items))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "dolist");
        let (_, violations) = bodies("(dotimes (i n))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "dotimes");
    }

    #[test]
    fn does_not_flag_a_form_with_a_body() {
        let (count, violations) = bodies("(when ready (go))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_dolist_with_a_body() {
        let (_, violations) = bodies("(dolist (x items) (print x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_form_missing_its_prefix() {
        // (when) has no test; that is a malformed form, not an empty body.
        let (_, violations) = bodies("(when)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = bodies("(cond (x))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = bodies("(WHEN ready)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_form_nested_in_a_body() {
        let (_, violations) = bodies("(defun f (x) (when x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(when ready)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_empty_bodies(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect empty bodies");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = bodies("(when ready)");
        let summary = summarize_empty_bodies(count, items);

        let quiet = evaluate_empty_body_policy(EmptyBodyPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_empty_body_policy(EmptyBodyPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
