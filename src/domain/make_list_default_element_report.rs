//! Common Lisp redundant-`make-list`-`:initial-element`-`nil` detection: a call
//! `(make-list n :initial-element nil)`. CLHS defines `make-list` as
//! `make-list size &key initial-element` with `initial-element` defaulting to
//! `nil`, so `(make-list n :initial-element nil)` restates the default and is
//! exactly `(make-list n)`.
//!
//! Only a bare `nil` literal value (no reader prefixes) is flagged; a non-`nil`
//! `:initial-element` (`(make-list n :initial-element 0)`) is meaningful and left
//! alone.
//!
//! The fix deletes the redundant ` :initial-element nil` argument pair (from the
//! end of the preceding argument through the `nil`), leaving the rest
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

/// Whether `view` is the bare `:initial-element` keyword atom.
fn is_initial_element_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":initial-element"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct MakeListDefaultElementItem {
    pub path: PathBuf,
    /// The span of the whole `(make-list …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :initial-element nil` argument pair.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct MakeListDefaultElementSummary {
    pub call_form_count: usize,
    pub violations: Vec<MakeListDefaultElementItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MakeListDefaultElementPolicyOptions {
    fail_on_violation: bool,
}

impl MakeListDefaultElementPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct MakeListDefaultElementPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<MakeListDefaultElementItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("make-list") {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_initial_element_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(MakeListDefaultElementItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
        });
        return;
    }
}

/// Collects every `(make-list n :initial-element nil)` across a whole file,
/// along with the total number of `make-list` calls scanned.
pub fn collect_make_list_default_elements(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MakeListDefaultElementItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut call_form_count, &mut violations)
        });
    }
    Ok((call_form_count, violations))
}

pub fn summarize_make_list_default_elements(
    call_form_count: usize,
    violations: Vec<MakeListDefaultElementItem>,
) -> MakeListDefaultElementSummary {
    MakeListDefaultElementSummary {
        call_form_count,
        violations,
    }
}

pub fn evaluate_make_list_default_element_policy(
    options: MakeListDefaultElementPolicyOptions,
    summary: &MakeListDefaultElementSummary,
) -> MakeListDefaultElementPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MakeListDefaultElementPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_form_count: summary.call_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<MakeListDefaultElementItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_make_list_default_elements(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect make-list default elements")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_initial_element_nil() {
        let source = "(make-list n :initial-element nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :initial-element nil"
        );
    }

    #[test]
    fn does_not_flag_non_nil_element() {
        let (count, violations) = calls("(make-list n :initial-element 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_make_list() {
        let (_, violations) = calls("(make-list n)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(MAKE-LIST n :INITIAL-ELEMENT nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(setf x (make-list 5 :initial-element nil))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(make-list n :initial-element nil)", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_make_list_default_elements(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect make-list default elements");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(make-list n :initial-element nil)");
        let summary = summarize_make_list_default_elements(count, items);

        let quiet = evaluate_make_list_default_element_policy(
            MakeListDefaultElementPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_make_list_default_element_policy(
            MakeListDefaultElementPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
