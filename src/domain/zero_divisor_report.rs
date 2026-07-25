//! Common Lisp literal-zero-divisor detection: a division-family form with a
//! literal `0` in a divisor position, which always signals `division-by-zero` at
//! run time. `(/ x 0)`, `(mod x 0)`, `(rem x 0)`, `(floor x 0)`, … can never
//! succeed, so this is a report-only bug (there is no meaningful rewrite).
//!
//! Divisor positions depend on the operator:
//!
//!   - `/`: with one argument `(/ d)` is `1/d`, so the sole argument is the
//!     divisor; with two or more `(/ n d …)` every argument after the first is a
//!     divisor.
//!   - `mod`/`rem`: the second (and only other) argument is the divisor.
//!   - `floor`/`ceiling`/`truncate`/`round` and their `f`-variants: the optional
//!     second argument is the divisor, flagged only when present.
//!
//! Only the bare integer/ratio literal `0` is matched (a float `0.0` divisor is a
//! different, defined-in-some-impls story and is left alone), and a
//! reader-conditional operand is left alone.
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

/// Operators whose second argument is the (optional) divisor.
const QUOTIENT_OPS: [&str; 10] = [
    "mod",
    "rem",
    "floor",
    "ceiling",
    "truncate",
    "round",
    "ffloor",
    "fceiling",
    "ftruncate",
    "fround",
];

/// Whether `view` is the bare integer `0` literal (no reader prefixes; `0.0` is a
/// different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// The child indices that sit in a divisor position for `head` with `child_count`
/// children (index 0 is the operator).
fn divisor_indices(head: &str, child_count: usize) -> Vec<usize> {
    if head == "/" {
        return match child_count {
            // (/ d) -> 1/d: the sole argument is the divisor.
            2 => vec![1],
            // (/ n d ...) -> every argument after the numerator divides.
            n if n >= 3 => (2..n).collect(),
            _ => Vec::new(),
        };
    }
    if QUOTIENT_OPS.contains(&head) && child_count == 3 {
        // (op number divisor): the divisor is the second argument.
        return vec![2];
    }
    Vec::new()
}

#[derive(Debug, Clone)]
pub struct ZeroDivisorItem {
    pub path: PathBuf,
    /// The span of the whole division form.
    pub span: ByteSpan,
    /// The operator, lowercased.
    pub operator: String,
    /// The span of the literal `0` divisor.
    pub divisor_span: ByteSpan,
}

#[derive(Debug)]
pub struct ZeroDivisorSummary {
    pub division_form_count: usize,
    pub violations: Vec<ZeroDivisorItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ZeroDivisorPolicyOptions {
    fail_on_violation: bool,
}

impl ZeroDivisorPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ZeroDivisorPolicy {
    pub fail_on_violation: bool,
    pub division_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    division_form_count: &mut usize,
    violations: &mut Vec<ZeroDivisorItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let lower = head.to_ascii_lowercase();
    if lower != "/" && !QUOTIENT_OPS.contains(&lower.as_str()) {
        return;
    }
    *division_form_count += 1;

    for index in divisor_indices(&lower, view.children.len()) {
        let divisor = &view.children[index];
        if is_zero_literal(divisor) {
            violations.push(ZeroDivisorItem {
                path: path.to_path_buf(),
                span: view.span,
                operator: lower.clone(),
                divisor_span: divisor.span,
            });
            return;
        }
    }
}

/// Collects every division-family form with a literal `0` divisor across a whole
/// file, along with the total number of division forms scanned.
pub fn collect_zero_divisors(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ZeroDivisorItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut division_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut division_form_count, &mut violations)
        });
    }
    Ok((division_form_count, violations))
}

pub fn summarize_zero_divisors(
    division_form_count: usize,
    violations: Vec<ZeroDivisorItem>,
) -> ZeroDivisorSummary {
    ZeroDivisorSummary {
        division_form_count,
        violations,
    }
}

pub fn evaluate_zero_divisor_policy(
    options: ZeroDivisorPolicyOptions,
    summary: &ZeroDivisorSummary,
) -> ZeroDivisorPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ZeroDivisorPolicy {
        fail_on_violation: options.fail_on_violation(),
        division_form_count: summary.division_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn divs(input: &str) -> (usize, Vec<ZeroDivisorItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_zero_divisors(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect zero divisors")
    }

    #[test]
    fn flags_divide_by_zero() {
        let (count, violations) = divs("(/ x 0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "/");
    }

    #[test]
    fn flags_reciprocal_of_zero() {
        // (/ 0) is 1/0.
        let (_, violations) = divs("(/ 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_later_zero_divisor() {
        let (_, violations) = divs("(/ n a 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_zero_numerator() {
        // (/ 0 x) is 0, perfectly fine.
        let (_, violations) = divs("(/ 0 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_mod_rem_zero() {
        assert_eq!(divs("(mod x 0)").1.len(), 1);
        assert_eq!(divs("(rem x 0)").1.len(), 1);
    }

    #[test]
    fn flags_quotient_op_zero_divisor() {
        assert_eq!(divs("(floor x 0)").1.len(), 1);
        assert_eq!(divs("(truncate x 0)").1.len(), 1);
    }

    #[test]
    fn does_not_flag_single_arg_quotient() {
        // (floor x) has no divisor argument.
        assert!(divs("(floor x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_nonzero_or_float_divisor() {
        assert!(divs("(/ x 2)").1.is_empty());
        assert!(divs("(/ x 0.0)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        assert_eq!(divs("(MOD x 0)").1.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(/ x 0)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_zero_divisors(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect zero divisors");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = divs("(/ x 0)");
        let summary = summarize_zero_divisors(count, items);

        let quiet = evaluate_zero_divisor_policy(ZeroDivisorPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_zero_divisor_policy(ZeroDivisorPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
