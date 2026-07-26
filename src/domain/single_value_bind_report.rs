//! Common Lisp single-value `multiple-value-bind` detection: a
//! `multiple-value-bind` whose variable list names exactly one variable.
//! `multiple-value-bind` binds each variable to the corresponding value of the
//! form; `let` binds its variable to the form's *primary* value. Those coincide
//! for a single variable, so `(multiple-value-bind (x) form body)` is exactly
//! `(let ((x form)) body)` — same binding, same body, same result — but without
//! the multiple-values machinery the reader must otherwise account for.
//!
//! Only the one-variable shape is flagged. A binding list with two or more
//! variables genuinely captures secondary values that `let` would discard, and
//! an empty variable list `()` is a `progn`, not a `let` — both are left alone.
//! A `&optional`/lambda-list-keyword pseudo-variable and a reader-conditional
//! operand are left alone as well.
//!
//! The fix rewrites `(multiple-value-bind (x) form body…)` as
//! `(let ((x form)) body…)`, copying the variable, form, and body from their
//! exact source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteOffset, ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled shape.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is a plain variable name: a bare symbol atom that is not a
/// lambda-list keyword (`&optional`, …) and not a reader-conditional operand.
fn is_plain_variable(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| !text.is_empty() && !text.starts_with('&'))
}

#[derive(Debug, Clone)]
pub struct SingleValueBindItem {
    pub path: PathBuf,
    /// The span of the whole `(multiple-value-bind (x) form body…)` form.
    pub span: ByteSpan,
    /// The span of the single bound variable `x`.
    pub var_span: ByteSpan,
    /// The span of the value form.
    pub form_span: ByteSpan,
    /// The span covering the body forms (`None` when there is no body).
    pub body_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct SingleValueBindSummary {
    pub bind_form_count: usize,
    pub violations: Vec<SingleValueBindItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SingleValueBindPolicyOptions {
    fail_on_violation: bool,
}

impl SingleValueBindPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct SingleValueBindPolicy {
    pub fail_on_violation: bool,
    pub bind_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_bind(
    view: &ExpressionView,
    path: &Path,
    bind_form_count: &mut usize,
    violations: &mut Vec<SingleValueBindItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("multiple-value-bind") {
        return;
    }
    *bind_form_count += 1;

    // children: [multiple-value-bind, varlist, form, body…] — need at least the
    // variable list and the value form.
    if view.children.len() < 3 {
        return;
    }
    let varlist = &view.children[1];
    if !is_paren_list(varlist) {
        return;
    }
    // Exactly one bound variable, and it must be a plain symbol.
    if varlist.children.len() != 1 {
        return;
    }
    let var = &varlist.children[0];
    if !is_plain_variable(var) {
        return;
    }
    let form = &view.children[2];
    if is_reader_conditional(form) {
        return;
    }

    // Body spans from the first body form through the last (verbatim copy keeps
    // its spacing and comments); `None` when there is no body.
    let body_span = if view.children.len() > 3 {
        let start = view.children[3].span.start();
        let end = view.children[view.children.len() - 1].span.end();
        Some(ByteSpan::new(
            ByteOffset::new(start.get()),
            ByteOffset::new(end.get()),
        ))
    } else {
        None
    };

    violations.push(SingleValueBindItem {
        path: path.to_path_buf(),
        span: view.span,
        var_span: var.span,
        form_span: form.span,
        body_span,
    });
}

/// Collects every single-value `multiple-value-bind` across a whole file, along
/// with the total number of `multiple-value-bind` forms scanned.
pub fn collect_single_value_binds(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SingleValueBindItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut bind_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_bind(subview, path, &mut bind_form_count, &mut violations);
        });
    }
    Ok((bind_form_count, violations))
}

pub fn summarize_single_value_binds(
    bind_form_count: usize,
    violations: Vec<SingleValueBindItem>,
) -> SingleValueBindSummary {
    SingleValueBindSummary {
        bind_form_count,
        violations,
    }
}

pub fn evaluate_single_value_bind_policy(
    options: SingleValueBindPolicyOptions,
    summary: &SingleValueBindSummary,
) -> SingleValueBindPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SingleValueBindPolicy {
        fail_on_violation: options.fail_on_violation(),
        bind_form_count: summary.bind_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binds(input: &str) -> (usize, Vec<SingleValueBindItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_single_value_binds(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect single-value binds")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_variable() {
        let source = "(multiple-value-bind (q) (truncate a b) (use q))";
        let (count, violations) = binds(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].var_span), "q");
        assert_eq!(slice(source, violations[0].form_span), "(truncate a b)");
        let body = violations[0].body_span.expect("body span");
        assert_eq!(slice(source, body), "(use q)");
    }

    #[test]
    fn flags_multi_form_body() {
        let source = "(multiple-value-bind (x) (f) a b c)";
        let (_, violations) = binds(source);
        let body = violations[0].body_span.expect("body span");
        assert_eq!(slice(source, body), "a b c");
    }

    #[test]
    fn flags_empty_body() {
        let (_, violations) = binds("(multiple-value-bind (x) (f))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].body_span.is_none());
    }

    #[test]
    fn does_not_flag_two_variables() {
        let (count, violations) = binds("(multiple-value-bind (q r) (truncate a b) q)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_variable_list() {
        // (multiple-value-bind () form body) is a progn, not a let.
        let (_, violations) = binds("(multiple-value-bind () (side-effect) done)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_lambda_list_keyword() {
        let (_, violations) = binds("(multiple-value-bind (&rest xs) (f) xs)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = binds("(MULTIPLE-VALUE-BIND (x) (f) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_bind() {
        let (_, violations) = binds("(defun g () (multiple-value-bind (v) (compute) (list v)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(multiple-value-bind (x) (f) x)", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_single_value_binds(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect single-value binds");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = binds("(multiple-value-bind (x) (f) x)");
        let summary = summarize_single_value_binds(count, items);

        let quiet =
            evaluate_single_value_bind_policy(SingleValueBindPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_single_value_bind_policy(SingleValueBindPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
