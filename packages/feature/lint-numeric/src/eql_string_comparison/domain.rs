//! Common Lisp `eq`/`eql`-on-a-string detection: a call to `eq` or `eql`
//! with a string-literal argument, such as `(eql name "root")` or
//! `(eq (status x) "done")`. Two strings are `eql` only if they are the
//! *same object*, which distinct string literals never are, so such a test
//! is almost always false regardless of the string's contents — a classic
//! Common Lisp bug where `string=` or `equal` was intended.
//!
//! Character literals (`#\a`) are excluded: characters *are* reliably `eql`,
//! so `(eql c #\a)` is correct and common. Only string literals (`"..."`)
//! trigger the report.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`], since such a call can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

const EQ_HEADS: [&str; 2] = ["eq", "eql"];

fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

#[derive(Debug, Clone)]
pub struct EqlStringComparisonItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub literal: String,
}

#[derive(Debug)]
pub struct EqlStringComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<EqlStringComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EqlStringComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl EqlStringComparisonPolicyOptions {
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
pub struct EqlStringComparisonPolicy {
    pub fail_on_violation: bool,
    pub comparison_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn examine_comparison(
    view: &ExpressionView,
    path: &Path,
    comparison_form_count: &mut usize,
    violations: &mut Vec<EqlStringComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EQ_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *comparison_form_count += 1;

    // Report the first string-literal argument (after the operator); a call
    // with two string literals is still one bug, not two.
    if let Some(literal) = view
        .children
        .iter()
        .skip(1)
        .find(|argument| is_string_literal(argument))
    {
        violations.push(EqlStringComparisonItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            literal: atom_text(literal).unwrap_or_default().to_owned(),
        });
    }
}

/// Collects every `eq`/`eql` call with a string-literal argument across a
/// whole file, along with the total number of `eq`/`eql` calls scanned.
pub fn collect_eql_string_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<EqlStringComparisonItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, path, &mut comparison_form_count, &mut violations);
        });
    }
    Ok((comparison_form_count, violations))
}

#[must_use]
pub const fn summarize_eql_string_comparisons(
    comparison_form_count: usize,
    violations: Vec<EqlStringComparisonItem>,
) -> EqlStringComparisonSummary {
    EqlStringComparisonSummary {
        comparison_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_eql_string_comparison_policy(
    options: EqlStringComparisonPolicyOptions,
    summary: &EqlStringComparisonSummary,
) -> EqlStringComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EqlStringComparisonPolicy {
        fail_on_violation: options.fail_on_violation(),
        comparison_form_count: summary.comparison_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparisons(input: &str) -> (usize, Vec<EqlStringComparisonItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_eql_string_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect eql string comparisons")
    }

    #[test]
    fn flags_eql_against_a_string_literal() {
        let (comparison_form_count, violations) = comparisons("(eql name \"root\")");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eql");
        assert_eq!(violations[0].literal, "\"root\"");
    }

    #[test]
    fn flags_eq_against_a_string_literal() {
        let (_, violations) = comparisons("(eq (status x) \"done\")");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn does_not_flag_eql_against_a_character_literal() {
        let (comparison_form_count, violations) = comparisons("(eql c #\\a)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eql_between_two_symbols() {
        let (comparison_form_count, violations) = comparisons("(eql x y)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equal_or_string_equal() {
        let (comparison_form_count, violations) = comparisons("(equal x \"root\")");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) =
            comparisons("(defun f (x) (when (eq x \"y\") 1))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(eq x \"root\")").expect("parse input");
        let (comparison_form_count, violations) =
            collect_eql_string_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect eql string comparisons");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (comparison_form_count, items) = comparisons("(eql x \"root\")");
        let summary = summarize_eql_string_comparisons(comparison_form_count, items);

        let quiet = evaluate_eql_string_comparison_policy(
            EqlStringComparisonPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_eql_string_comparison_policy(
            EqlStringComparisonPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
