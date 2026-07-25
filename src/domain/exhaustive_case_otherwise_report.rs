//! Common Lisp exhaustive-case-otherwise detection: an `ecase`, `ccase`,
//! `etypecase`, or `ctypecase` form with a `t` or `otherwise` clause. These
//! are the *exhaustive* case forms — they signal an error when no clause
//! matches, so a default clause is not permitted (CLHS: "no explicit
//! otherwise or t clause is permitted"). Writing one is almost always a bug: a
//! `case` copied into an `ecase`, silently defeating the exhaustiveness check.
//!
//! The catch-all must be a *bare* `t`/`otherwise` key designator; a key list
//! such as `((t) …)` matches the literal symbol `T` and is not a default, so
//! it is not flagged — the same distinction as
//! [`crate::domain::unreachable_case_clause_report`], which handles the
//! non-exhaustive `case`/`typecase` where a default *is* permitted.
//!
//! Forms whose clause structure is not statically visible are skipped: a
//! quoted/quasiquoted form, and clauses guarded by a `#+`/`#-` reader
//! conditional (which parse as opaque atoms, not list clauses).
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

const EXHAUSTIVE_HEADS: [&str; 4] = ["ecase", "ccase", "etypecase", "ctypecase"];

/// The bare `t`/`otherwise` catch-all designator of a clause, if any. A key
/// *list* (`(t)`) matches the literal symbol and is not a catch-all, so the
/// first child must be an unprefixed atom.
fn catch_all_designator(clause: &ExpressionView) -> Option<String> {
    let key = clause.children.first()?;
    if !key.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(key)?;
    if text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("otherwise") {
        Some(text.to_owned())
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ExhaustiveCaseOtherwiseItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub designator: String,
}

#[derive(Debug)]
pub struct ExhaustiveCaseOtherwiseSummary {
    pub case_form_count: usize,
    pub violations: Vec<ExhaustiveCaseOtherwiseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExhaustiveCaseOtherwisePolicyOptions {
    fail_on_violation: bool,
}

impl ExhaustiveCaseOtherwisePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ExhaustiveCaseOtherwisePolicy {
    pub fail_on_violation: bool,
    pub case_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_case(
    view: &ExpressionView,
    path: &Path,
    case_form_count: &mut usize,
    violations: &mut Vec<ExhaustiveCaseOtherwiseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EXHAUSTIVE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2. A feature-conditional
    // clause reads as an opaque atom (not a list) and is skipped.
    for clause in view.children.iter().skip(2) {
        if !is_paren_list(clause) {
            continue;
        }
        if let Some(designator) = catch_all_designator(clause) {
            violations.push(ExhaustiveCaseOtherwiseItem {
                path: path.to_path_buf(),
                span: clause.span,
                head: head.to_owned(),
                designator,
            });
        }
    }
}

/// Collects every exhaustive case form (`ecase`/`ccase`/`etypecase`/
/// `ctypecase`) with a forbidden `t`/`otherwise` clause across a whole file,
/// along with the total number of such forms scanned.
pub fn collect_exhaustive_case_otherwise(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ExhaustiveCaseOtherwiseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, path, &mut case_form_count, &mut violations)
        });
    }
    Ok((case_form_count, violations))
}

pub fn summarize_exhaustive_case_otherwise(
    case_form_count: usize,
    violations: Vec<ExhaustiveCaseOtherwiseItem>,
) -> ExhaustiveCaseOtherwiseSummary {
    ExhaustiveCaseOtherwiseSummary {
        case_form_count,
        violations,
    }
}

pub fn evaluate_exhaustive_case_otherwise_policy(
    options: ExhaustiveCaseOtherwisePolicyOptions,
    summary: &ExhaustiveCaseOtherwiseSummary,
) -> ExhaustiveCaseOtherwisePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ExhaustiveCaseOtherwisePolicy {
        fail_on_violation: options.fail_on_violation(),
        case_form_count: summary.case_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<ExhaustiveCaseOtherwiseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_exhaustive_case_otherwise(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect exhaustive case otherwise")
    }

    #[test]
    fn flags_a_t_clause_in_ecase() {
        let (form_count, items) = violations("(ecase x (1 :a) (t :b))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "ecase");
        assert_eq!(items[0].designator, "t");
    }

    #[test]
    fn flags_an_otherwise_clause_in_etypecase() {
        let (_, items) = violations("(etypecase x (integer 1) (otherwise 2))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "etypecase");
        assert_eq!(items[0].designator, "otherwise");
    }

    #[test]
    fn flags_a_ccase_and_ctypecase() {
        let (_, ccase) = violations("(ccase x (1 :a) (otherwise :b))");
        assert_eq!(ccase.len(), 1);
        let (_, ctypecase) = violations("(ctypecase x (integer 1) (t 2))");
        assert_eq!(ctypecase.len(), 1);
    }

    #[test]
    fn does_not_flag_a_default_in_case() {
        // `case` permits a t/otherwise default; only the exhaustive forms don't.
        let (form_count, items) = violations("(case x (1 :a) (t :b))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_literal_t_key_list() {
        let (_, items) = violations("(ecase x ((t) :sym) (1 :a))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_normal_ecase() {
        let (form_count, items) = violations("(ecase x (1 :a) (2 :b) (3 :c))");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (form_count, items) = violations("(list '(ecase x (t 1)))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (form_count, items) = violations("(defun f (x) (ecase x (1 :a) (otherwise :b)))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(ecase x (t 1))", Dialect::Clojure)
            .expect("parse input");
        let (form_count, items) =
            collect_exhaustive_case_otherwise(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect exhaustive case otherwise");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (form_count, items) = violations("(ecase x (t 1))");
        let summary = summarize_exhaustive_case_otherwise(form_count, items);

        let quiet = evaluate_exhaustive_case_otherwise_policy(
            ExhaustiveCaseOtherwisePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_exhaustive_case_otherwise_policy(
            ExhaustiveCaseOtherwisePolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
