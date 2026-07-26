//! Common Lisp redundant-`prog1` detection: a `(prog1 x)` wrapping a single
//! form. `prog1` evaluates its forms left to right and returns the value(s) of
//! the *first*; with only one form there is nothing else to sequence, so
//! `(prog1 x)` is exactly `x` — same value(s), same single evaluation.
//!
//! Only the exact single-form shape is matched. A `(prog1 x y …)` with trailing
//! forms genuinely sequences side effects and is left alone, as is an empty
//! `(prog1)` (which is `nil`, a different rewrite) and a reader-conditional body
//! (build-dependent).
//!
//! Unlike `redundant-progn`, `prog1` returns the value of its *first* form,
//! which is why this is its own rule: a multi-form `prog1` cannot become
//! `progn` (that would return the last form instead).
//!
//! The fix replaces the whole form with its single inner form's source, so the
//! rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled body.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantProg1Item {
    pub path: PathBuf,
    /// The span of the whole `(prog1 x)` form.
    pub span: ByteSpan,
    /// The span of the single inner form (for reconstructing the fix).
    pub form_span: ByteSpan,
}

#[derive(Debug)]
pub struct RedundantProg1Summary {
    pub prog1_form_count: usize,
    pub violations: Vec<RedundantProg1Item>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantProg1PolicyOptions {
    fail_on_violation: bool,
}

impl RedundantProg1PolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantProg1Policy {
    pub fail_on_violation: bool,
    pub prog1_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    prog1_form_count: &mut usize,
    violations: &mut Vec<RedundantProg1Item>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("prog1") {
        return;
    }
    *prog1_form_count += 1;

    // children: [prog1, form] — require exactly one body form.
    if view.children.len() != 2 {
        return;
    }
    let form = &view.children[1];
    if is_reader_conditional(form) {
        return;
    }

    violations.push(RedundantProg1Item {
        path: path.to_path_buf(),
        span: view.span,
        form_span: form.span,
    });
}

/// Collects every single-form `(prog1 x)` across a whole file, along with the
/// total number of `prog1` forms scanned.
pub fn collect_redundant_prog1s(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantProg1Item>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut prog1_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut prog1_form_count, &mut violations);
        });
    }
    Ok((prog1_form_count, violations))
}

#[must_use]
pub const fn summarize_redundant_prog1s(
    prog1_form_count: usize,
    violations: Vec<RedundantProg1Item>,
) -> RedundantProg1Summary {
    RedundantProg1Summary {
        prog1_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_prog1_policy(
    options: RedundantProg1PolicyOptions,
    summary: &RedundantProg1Summary,
) -> RedundantProg1Policy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantProg1Policy {
        fail_on_violation: options.fail_on_violation(),
        prog1_form_count: summary.prog1_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prog1s(input: &str) -> (usize, Vec<RedundantProg1Item>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_prog1s(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant prog1s")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_form_prog1() {
        let source = "(prog1 (compute))";
        let (count, violations) = prog1s(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn flags_atom_form() {
        let source = "(prog1 x)";
        let (_, violations) = prog1s(source);
        assert_eq!(slice(source, violations[0].form_span), "x");
    }

    #[test]
    fn does_not_flag_multi_form_prog1() {
        // (prog1 a b) sequences side effects and returns a; not redundant.
        let (count, violations) = prog1s("(prog1 a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_prog1() {
        let (_, violations) = prog1s("(prog1)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = prog1s("(PROG1 x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = prog1s("(defun f (x) (prog1 x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(prog1 x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_prog1s(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant prog1s");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = prog1s("(prog1 x)");
        let summary = summarize_redundant_prog1s(count, items);

        let quiet =
            evaluate_redundant_prog1_policy(RedundantProg1PolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_prog1_policy(RedundantProg1PolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
