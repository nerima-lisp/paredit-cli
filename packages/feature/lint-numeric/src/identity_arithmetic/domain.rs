//! Common Lisp identity-arithmetic detection: an arithmetic form with a
//! redundant identity operand — `(+ x 0)`, `(* x 1)`, `(- x 0)`, `(/ x 1)`.
//! Adding `0`, multiplying by `1`, subtracting `0`, or dividing by `1` returns
//! the other operand unchanged, so the identity literal is pure noise, common
//! after mechanical macro expansion or when operands are edited away.
//!
//! Only the *integer* identity literals `0` and `1` are flagged — `0.0`/`1.0`
//! coerce the result to a float and are meaningful, so they are left alone. For
//! `+` and `*` (commutative) the identity may be in any operand position; for
//! `-` and `/` only a *non-first* operand is an identity (`(- 0 x)` negates and
//! `(/ 1 x)` is a reciprocal, so a leading `0`/`1` is not redundant).
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// For an arithmetic head, the identity literal that is redundant and the first
/// operand index at which it may appear (1 for the commutative `+`/`*`, 2 for
/// `-`/`/` whose leading operand is not an identity position).
fn identity_for(head: &str) -> Option<(&'static str, usize)> {
    match head {
        "+" => Some(("0", 1)),
        "*" => Some(("1", 1)),
        "-" => Some(("0", 2)),
        "/" => Some(("1", 2)),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct IdentityArithmeticItem {
    pub span: ByteSpan,
    /// The arithmetic operator (`+`, `-`, `*`, or `/`).
    pub operator: String,
    /// The redundant identity literal (`0` or `1`).
    pub identity: &'static str,
}

impl Finding for IdentityArithmeticItem {
    /// The rule's own name rather than one of its four operators.
    ///
    /// The operator would be the natural variant — it is a closed set, and the
    /// item already carries it — but `+`, `-`, `*` and `/` are punctuation, and
    /// `kind` is pasted into a SARIF `ruleId` (`inspect/identity-arithmetic/…`)
    /// and a CSV column. A rule id ending in `/` is worse than no split at all,
    /// so the operator stays a JSON field and a text column.
    fn kind(&self) -> &'static str {
        "identity-arithmetic"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone(), format!("identity={}", self.identity)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("identity", json!(self.identity)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} has a redundant identity operand {} (it does nothing)",
            self.operator, self.identity
        )
    }
}

pub fn examine_form(
    view: &ExpressionView,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<IdentityArithmeticItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    // The operators are single ASCII glyphs; an exact match is enough.
    let Some((identity, first_index)) = identity_for(head) else {
        return;
    };
    *arithmetic_form_count += 1;

    // Report the first identity operand at an identity position; one form with
    // two redundant identities is still one form's redundancy.
    let has_identity = view
        .children
        .iter()
        .skip(first_index)
        .any(|child| atom_text(child) == Some(identity));
    if has_identity {
        violations.push(IdentityArithmeticItem {
            span: view.span,
            operator: head.to_owned(),
            identity,
        });
    }
}

/// Collects every arithmetic form with a redundant identity operand in one
/// file, with the number of arithmetic forms scanned as the denominator beside
/// them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_identity_arithmetic_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IdentityArithmeticItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("arithmetic_form_count", json!(0))],
        ));
    }

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, &mut arithmetic_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("arithmetic_form_count", json!(arithmetic_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<IdentityArithmeticItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_identity_arithmetic_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build identity arithmetic report")
    }

    fn forms(input: &str) -> (u64, Vec<IdentityArithmeticItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "arithmetic_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("arithmetic_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_add_zero() {
        let (count, violations) = forms("(+ x 0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "+");
        assert_eq!(violations[0].identity, "0");
    }

    #[test]
    fn flags_add_zero_in_any_position() {
        let (_, violations) = forms("(+ 0 x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_multiply_one() {
        let (_, violations) = forms("(* x 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].identity, "1");
    }

    #[test]
    fn flags_subtract_zero() {
        let (_, violations) = forms("(- x 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_divide_by_one() {
        let (_, violations) = forms("(/ x 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_leading_zero_in_subtraction() {
        // (- 0 x) is negation, not an identity.
        let (_, violations) = forms("(- 0 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_leading_one_in_division() {
        // (/ 1 x) is a reciprocal, not an identity.
        let (_, violations) = forms("(/ 1 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_identities() {
        // (* x 1.0) coerces to a float; it is meaningful.
        let (_, violations) = forms("(* x 1.0)");
        assert!(violations.is_empty());
        let (_, violations) = forms("(+ x 0.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_identity_literals() {
        let (_, violations) = forms("(+ x 2)");
        assert!(violations.is_empty());
        let (_, violations) = forms("(* x 0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = forms("(max x 0)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_form_nested_in_a_body() {
        let (_, violations) = forms("(defun f (x) (list (+ x 0)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(+ x 0)", Dialect::Clojure).expect("parse");
        let report =
            build_identity_arithmetic_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build identity arithmetic report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(+ x 2)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_identity() {
        let report = report("(defun f (x)\n  (* x 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "identity-arithmetic");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("*")), ("identity", json!("1"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["*".to_owned(), "identity=1".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_arithmetic_form_scanned_not_only_the_flagged_ones() {
        let report = report("(+ x 0)\n(+ x 2)\n(* y 3)\n");
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
