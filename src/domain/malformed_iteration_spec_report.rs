//! Common Lisp malformed-iteration-spec detection: a `dolist` or `dotimes`
//! binding spec that is not a `(var form [result])` list. `dolist`'s spec is
//! `(var list-form [result-form])` and `dotimes`'s is
//! `(var count-form [result-form])` — each takes exactly two or three elements.
//! A non-list spec (`(dolist x …)`), a one-element spec (`(dolist (x) …)`,
//! missing the list/count form), or a four-plus-element spec is a program
//! error, caught only at macroexpansion rather than by the reader.
//!
//! Scoped to `dolist`/`dotimes` on purpose: `do`/`do*` step bindings take a
//! third `step` form and a different overall shape, so they are not inspected
//! here.
//!
//! Forms whose spec arity is not statically visible are skipped to avoid false
//! positives: a quoted/quasiquoted form (data or a template), and any spec
//! whose value is or contains a `#+`/`#-` reader conditional or `,@` splice,
//! where the written element count differs from the evaluated one.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::render_expression;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

const ITERATION_HEADS: [&str; 2] = ["dolist", "dotimes"];

/// Whether a form's value can change how many elements actually reach the
/// evaluator, making a static arity count unreliable: a `,@` splice or Clojure
/// reader conditional (modeled as a prefix), or a Common Lisp `#+`/`#-`
/// conditional (modeled as an atom whose text begins `#+`/`#-`).
fn is_arity_ambiguous(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct MalformedIterationSpecItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub spec: String,
    pub element_count: usize,
}

#[derive(Debug)]
pub struct MalformedIterationSpecSummary {
    pub iteration_form_count: usize,
    pub violations: Vec<MalformedIterationSpecItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MalformedIterationSpecPolicyOptions {
    fail_on_violation: bool,
}

impl MalformedIterationSpecPolicyOptions {
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
pub struct MalformedIterationSpecPolicy {
    pub fail_on_violation: bool,
    pub iteration_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_iteration(
    view: &ExpressionView,
    path: &Path,
    iteration_form_count: &mut usize,
    violations: &mut Vec<MalformedIterationSpecItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !ITERATION_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted iteration form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    let Some(spec) = view.children.get(1) else {
        return;
    };
    *iteration_form_count += 1;

    // A spec that reads as a `#+`/`#-`-guarded node has no statically visible
    // shape.
    if is_arity_ambiguous(spec) {
        return;
    }

    if !is_paren_list(spec) {
        // A bare atom (or bracket/brace form) is never a valid spec.
        violations.push(MalformedIterationSpecItem {
            path: path.to_path_buf(),
            span: spec.span,
            head: head.to_owned(),
            spec: render_expression(spec),
            element_count: spec.children.len(),
        });
        return;
    }

    // A `#+`/`#-` element inside the spec makes its written arity unreliable.
    if spec.children.iter().any(is_arity_ambiguous) {
        return;
    }

    let element_count = spec.children.len();
    if !(2..=3).contains(&element_count) {
        violations.push(MalformedIterationSpecItem {
            path: path.to_path_buf(),
            span: spec.span,
            head: head.to_owned(),
            spec: render_expression(spec),
            element_count,
        });
    }
}

/// Collects every malformed `dolist`/`dotimes` spec across a whole file, along
/// with the total number of `dolist`/`dotimes` forms scanned.
pub fn collect_malformed_iteration_specs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MalformedIterationSpecItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut iteration_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_iteration(subview, path, &mut iteration_form_count, &mut violations);
        });
    }
    Ok((iteration_form_count, violations))
}

#[must_use]
pub const fn summarize_malformed_iteration_specs(
    iteration_form_count: usize,
    violations: Vec<MalformedIterationSpecItem>,
) -> MalformedIterationSpecSummary {
    MalformedIterationSpecSummary {
        iteration_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_malformed_iteration_spec_policy(
    options: MalformedIterationSpecPolicyOptions,
    summary: &MalformedIterationSpecSummary,
) -> MalformedIterationSpecPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MalformedIterationSpecPolicy {
        fail_on_violation: options.fail_on_violation(),
        iteration_form_count: summary.iteration_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(input: &str) -> (usize, Vec<MalformedIterationSpecItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_malformed_iteration_specs(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect malformed iteration specs")
    }

    #[test]
    fn flags_a_one_element_dolist_spec() {
        let (iteration_form_count, violations) = specs("(dolist (x) (print x))");
        assert_eq!(iteration_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 1);
        assert_eq!(violations[0].head, "dolist");
    }

    #[test]
    fn flags_a_four_element_dotimes_spec() {
        let (_, violations) = specs("(dotimes (i n r extra) (print i))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 4);
        assert_eq!(violations[0].head, "dotimes");
    }

    #[test]
    fn flags_a_non_list_spec() {
        let (_, violations) = specs("(dolist x (print x))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].spec, "x");
    }

    #[test]
    fn does_not_flag_a_two_element_spec() {
        let (iteration_form_count, violations) = specs("(dolist (x items) (print x))");
        assert_eq!(iteration_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_three_element_spec() {
        let (_, violations) = specs("(dolist (x items result) (print x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_valid_dotimes() {
        let (_, violations) = specs("(dotimes (i 10 done) (print i))");
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_feature_conditional_spec_element() {
        // `#+a n1 #-a n2 result` reads as feature-conditional atoms; only one
        // count form survives, so the written arity is not reliable.
        let (iteration_form_count, violations) =
            specs("(dotimes (i #+sbcl n1 #-sbcl n2 result) (print i))");
        assert_eq!(iteration_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_iteration_form() {
        let (iteration_form_count, violations) = specs("(list '(dolist (x) x))");
        assert_eq!(iteration_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_an_iteration_nested_in_a_function_body() {
        let (iteration_form_count, violations) = specs("(defun f (xs) (dolist (x) (print x)))");
        assert_eq!(iteration_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(dolist (x) (print x))", Dialect::Clojure)
            .expect("parse input");
        let (iteration_form_count, violations) =
            collect_malformed_iteration_specs(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect malformed iteration specs");
        assert_eq!(iteration_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (iteration_form_count, items) = specs("(dolist (x) (print x))");
        let summary = summarize_malformed_iteration_specs(iteration_form_count, items);

        let quiet = evaluate_malformed_iteration_spec_policy(
            MalformedIterationSpecPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_malformed_iteration_spec_policy(
            MalformedIterationSpecPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
