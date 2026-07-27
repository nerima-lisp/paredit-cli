//! Common Lisp redundant-divisor detection: a two-argument quotient operation
//! whose divisor is the literal integer `1`. `floor`, `ceiling`, `truncate`,
//! `round` and their float-returning variants `ffloor`, `fceiling`, `ftruncate`,
//! `fround` all take an optional `divisor` that defaults to `1`, so
//! `(floor x 1)` is exactly `(floor x)` — same quotient, same remainder, same
//! two returned values. Dropping the redundant `1` states the unit divisor the
//! way the operator was designed to express it.
//!
//! Only the bare integer literal `1` is matched. A float `1.0` is left alone:
//! `(floor 3 1.0)` returns a float remainder `0.0` while `(floor 3)` returns the
//! integer `0`, so the two are *not* equivalent. A non-`1` divisor, a
//! `#x1`/prefixed spelling, a variable divisor, and a reader-conditional operand
//! are all left alone. `mod`/`rem` are excluded — they require two arguments and
//! have no defaultable divisor.
//!
//! The fix rewrites `(floor x 1)` as `(floor x)`, copying the operator and
//! number operand from their exact source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// The quotient operators whose divisor defaults to `1`.
const QUOTIENT_OPS: [&str; 8] = [
    "floor",
    "ceiling",
    "truncate",
    "round",
    "ffloor",
    "fceiling",
    "ftruncate",
    "fround",
];

/// Whether `view` is the bare integer `1` literal (no reader prefixes, so `#x1`
/// and a prefixed `,1` are excluded; `1.0` is a different spelling, excluded).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("1")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantDivisorItem {
    pub path: PathBuf,
    /// The span of the whole `(floor x 1)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`floor`, `ceiling`, ...).
    pub operator: &'static str,
    /// The span of the operator token (preserves the source casing).
    pub operator_span: ByteSpan,
    /// The span of the number operand (for reconstructing the fix).
    pub number_span: ByteSpan,
}

#[derive(Debug)]
pub struct RedundantDivisorSummary {
    pub quotient_form_count: usize,
    pub violations: Vec<RedundantDivisorItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantDivisorPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantDivisorPolicyOptions {
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
pub struct RedundantDivisorPolicy {
    pub fail_on_violation: bool,
    pub quotient_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// The canonical (lowercase) operator name if `head` is a quotient operator.
fn quotient_operator(head: &str) -> Option<&'static str> {
    QUOTIENT_OPS
        .iter()
        .copied()
        .find(|op| head.eq_ignore_ascii_case(op))
}

pub fn examine(
    view: &ExpressionView,
    path: &Path,
    quotient_form_count: &mut usize,
    violations: &mut Vec<RedundantDivisorItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = quotient_operator(head) else {
        return;
    };
    *quotient_form_count += 1;

    // children: [op, number, divisor] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let number = &view.children[1];
    let divisor = &view.children[2];
    if is_reader_conditional(number) || is_reader_conditional(divisor) {
        return;
    }
    if !is_one_literal(divisor) {
        return;
    }

    violations.push(RedundantDivisorItem {
        path: path.to_path_buf(),
        span: view.span,
        operator,
        operator_span: view.children[0].span,
        number_span: number.span,
    });
}

/// Collects every `(op x 1)` quotient form across a whole file, along with the
/// total number of quotient forms scanned.
pub fn collect_redundant_divisors(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantDivisorItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut quotient_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut quotient_form_count, &mut violations);
        });
    }
    Ok((quotient_form_count, violations))
}

#[must_use]
pub const fn summarize_redundant_divisors(
    quotient_form_count: usize,
    violations: Vec<RedundantDivisorItem>,
) -> RedundantDivisorSummary {
    RedundantDivisorSummary {
        quotient_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_divisor_policy(
    options: RedundantDivisorPolicyOptions,
    summary: &RedundantDivisorSummary,
) -> RedundantDivisorPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantDivisorPolicy {
        fail_on_violation: options.fail_on_violation(),
        quotient_form_count: summary.quotient_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quotients(input: &str) -> (usize, Vec<RedundantDivisorItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_divisors(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant divisors")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_floor_by_one() {
        let source = "(floor total 1)";
        let (count, violations) = quotients(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "floor");
        assert_eq!(slice(source, violations[0].number_span), "total");
    }

    #[test]
    fn flags_every_quotient_operator() {
        for op in [
            "ceiling",
            "truncate",
            "round",
            "ffloor",
            "fceiling",
            "ftruncate",
            "fround",
        ] {
            let source = format!("({op} x 1)");
            let (_, violations) = quotients(&source);
            assert_eq!(violations.len(), 1, "expected {op} flagged");
            assert_eq!(violations[0].operator, op);
        }
    }

    #[test]
    fn preserves_operator_source_casing() {
        let source = "(FLOOR x 1)";
        let (_, violations) = quotients(source);
        assert_eq!(slice(source, violations[0].operator_span), "FLOOR");
    }

    #[test]
    fn does_not_flag_non_one_divisor() {
        let (count, violations) = quotients("(floor x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_one() {
        // (floor 3 1.0) yields a float remainder, unlike (floor 3).
        assert!(quotients("(floor x 1.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_one_argument_form() {
        // (floor x) already has no divisor.
        let (count, violations) = quotients("(floor x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_mod_or_rem() {
        assert!(quotients("(mod x 1)").1.is_empty());
        assert!(quotients("(rem x 1)").1.is_empty());
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = quotients("(defun f (x) (truncate x 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(floor x 1)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_divisors(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant divisors");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = quotients("(floor x 1)");
        let summary = summarize_redundant_divisors(count, items);

        let quiet =
            evaluate_redundant_divisor_policy(RedundantDivisorPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_divisor_policy(RedundantDivisorPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
