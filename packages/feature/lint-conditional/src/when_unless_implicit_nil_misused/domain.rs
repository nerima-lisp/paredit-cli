//! Common Lisp detection of a `when`/`unless` whose value is handed straight to
//! an operator that requires a number.
//!
//! `(when test form)` evaluates to `nil` when `test` is false — that is the
//! whole point of the macro, and it is what makes it a *statement* form. Using
//! its value as an argument to `+` therefore means "add `nil` to a number
//! whenever the test fails", which CLHS makes a `type-error`: every argument to
//! `+` must be a `number`, and `nil` is a symbol.
//!
//! ```lisp
//! (+ base (when discount-p discount))   ; type-error whenever discount-p is nil
//! (+ base (if discount-p discount 0))   ; what was meant
//! ```
//!
//! # Why this anchors on the arithmetic operator, not on `when`
//!
//! The defect is a property of the *pair* — a `when` in an argument position of
//! a strict operator — and a rule is handed one node with no parent pointer. A
//! rule anchored on `when` would have to find its enclosing form, and doing
//! that by re-scanning the file's top-level forms once per `when` is quadratic:
//! `when` is dense in ordinary code, and two rules in this repository that scan
//! per invocation account for most of a lint run on a large file.
//!
//! Anchoring on the arithmetic head inverts it into a purely local test. The
//! matched node *is* the call, its arguments are its own children, and the
//! check is "does any direct child's head spell `when` or `unless`" — no
//! ancestor walk, no whole-file scan, no allocation, and a rejection on the
//! first child for the overwhelming majority of arithmetic forms, whose
//! arguments are atoms.
//!
//! # What "strict" means here, and what it deliberately excludes
//!
//! [`STRICT_NUMERIC_HEADS`] lists only operators for which *every* argument
//! must be a number, so `nil` in any argument position is an error regardless
//! of the others. That excludes a great deal of plausible-sounding company:
//!
//! - **String and sequence operators are not strict.** `nil` is the empty list,
//!   which is a sequence, so `(length (when p "x"))` returns `0` and
//!   `(concatenate 'string "a" (when p "b"))` returns `"a"` — both are
//!   perfectly legal and neither is a defect. `(string-upcase nil)` returns
//!   `"NIL"`, because `nil` is a string designator. Reporting any of these
//!   would be a false positive, so the "string op" half of the obvious framing
//!   of this rule is simply wrong and is not implemented.
//! - **`incf`/`decf` are excluded** because their first argument is a *place*
//!   rather than a value, which would need a different rule for each position.
//! - **Only a direct child is reported.** `(+ x (or (when p 1) 0))` supplies a
//!   number in every case and is silent, which is also the idiom that repairs a
//!   real finding.
//!
//! # Limits
//!
//! A `macrolet`/`flet` that shadows `when` or `unless` locally would make this
//! a false positive; the shape is vanishingly rare and detecting it needs the
//! binding table, which this rule deliberately never asks for. Nothing here
//! tries to prove the test can actually be false — a `when` whose test is
//! constant is `constant-when-test`'s subject.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, has_reader_conditional_child, normalized_symbol};

/// Operators every one of whose arguments must be a `number`.
///
/// Kept to that one criterion on purpose: an operator that accepts `nil` in
/// *any* argument position — every sequence and string function, because `nil`
/// is the empty list and a string designator — would turn this rule into noise.
pub const STRICT_NUMERIC_HEADS: [&str; 21] = [
    "+", "-", "*", "/", "1+", "1-", "mod", "rem", "abs", "signum", "sqrt", "isqrt", "expt", "gcd",
    "lcm", "max", "min", "floor", "ceiling", "truncate", "round",
];

/// The two macros whose untaken branch is an implicit `nil`.
const IMPLICIT_NIL_HEADS: [&str; 2] = ["when", "unless"];

#[derive(Debug, Clone)]
pub struct ImplicitNilItem {
    /// The span of the offending `when`/`unless` argument.
    pub span: ByteSpan,
    /// The arithmetic operator receiving it, normalized.
    pub operator: String,
    /// `when` or `unless`.
    pub conditional: String,
    /// Which argument position it is, counting from 1.
    pub argument_index: usize,
}

impl Finding for ImplicitNilItem {
    fn kind(&self) -> &'static str {
        "when-unless-implicit-nil-misused"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.operator.clone(),
            self.conditional.clone(),
            self.argument_index.to_string(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("conditional", json!(self.conditional)),
            ("argument_index", json!(self.argument_index)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} yields nil when its test fails, and {} requires a number",
            self.conditional, self.operator
        )
    }
}

/// The `when`/`unless` head of an argument, if it has one.
///
/// A reader-prefixed argument (`'(when …)`, `#'(when …)`) is data or a function
/// designator, not a value flowing into the call.
fn implicit_nil_head(argument: &ExpressionView) -> Option<String> {
    if !argument.reader_prefixes.is_empty() {
        return None;
    }
    let head = list_head(argument)?;
    symbol_in(head, &IMPLICIT_NIL_HEADS).then(|| normalized_symbol(head))
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_arithmetic(
    view: &ExpressionView,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<ImplicitNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &STRICT_NUMERIC_HEADS) {
        return;
    }
    *arithmetic_form_count += 1;

    // A build-dependent argument list has no settled shape.
    if has_reader_conditional_child(view) {
        return;
    }

    let operator = normalized_symbol(head);
    for (offset, argument) in view.children.iter().skip(1).enumerate() {
        if let Some(conditional) = implicit_nil_head(argument) {
            violations.push(ImplicitNilItem {
                span: argument.span,
                operator: operator.clone(),
                conditional,
                argument_index: offset + 1,
            });
        }
    }
}

/// Collects every `when`/`unless` handed to a strict numeric operator in one
/// file, with the number of such operator forms scanned as the denominator.
pub fn build_when_unless_implicit_nil_misused_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ImplicitNilItem>> {
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
        for_each_evaluated_subview(&view, |subview| {
            examine_arithmetic(subview, &mut arithmetic_form_count, &mut violations);
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

    fn report(input: &str) -> FileFindings<ImplicitNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_when_unless_implicit_nil_misused_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build when-unless-implicit-nil-misused report")
    }

    fn findings(input: &str) -> Vec<ImplicitNilItem> {
        report(input).findings
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_when_in_an_addition() {
        let items = findings("(+ base (when discount-p discount))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "+");
        assert_eq!(items[0].conditional, "when");
        assert_eq!(items[0].argument_index, 2);
    }

    #[test]
    fn flags_an_unless_too() {
        let items = findings("(* n (unless empty-p factor))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].conditional, "unless");
    }

    #[test]
    fn flags_every_strict_operator() {
        for head in STRICT_NUMERIC_HEADS {
            let source = format!("({head} (when p 1))");
            assert_eq!(findings(&source).len(), 1, "{head}");
        }
    }

    #[test]
    fn flags_the_first_argument_position() {
        let items = findings("(- (when p 1) 2)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_index, 1);
    }

    #[test]
    fn flags_both_offending_arguments() {
        assert_eq!(findings("(+ (when a 1) (unless b 2))").len(), 2);
    }

    #[test]
    fn case_folds_and_strips_the_package_qualifier() {
        assert_eq!(findings("(CL:+ base (CL:WHEN p 1))").len(), 1);
    }

    #[test]
    fn finds_the_shape_nested_in_a_function_body() {
        assert_eq!(
            findings("(defun total (base p d) (+ base (when p d)))").len(),
            1
        );
    }

    // -- near-miss negatives: the false positives this rule must not make ----

    /// `nil` is the empty list, so a sequence operator accepts it happily.
    /// This is why the "string op" half of the obvious framing is not
    /// implemented.
    #[test]
    fn does_not_flag_a_sequence_or_string_operator() {
        assert!(findings(r#"(length (when p "x"))"#).is_empty());
        assert!(findings(r#"(concatenate 'string "a" (when p "b"))"#).is_empty());
        assert!(findings("(string-upcase (when p \"a\"))").is_empty());
        assert!(findings("(append a (when p (list 1)))").is_empty());
        assert!(findings("(list (when p 1))").is_empty());
    }

    /// The idiom that repairs a real finding must not itself be a finding.
    #[test]
    fn does_not_flag_a_when_guarded_by_or() {
        assert!(findings("(+ base (or (when p d) 0))").is_empty());
    }

    #[test]
    fn does_not_flag_an_if_which_always_supplies_a_number() {
        assert!(findings("(+ base (if p d 0))").is_empty());
    }

    #[test]
    fn does_not_flag_a_when_that_is_not_an_argument() {
        assert!(findings("(when p (+ 1 2))").is_empty());
        assert!(findings("(progn (when p 1) (+ 1 2))").is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_arithmetic() {
        assert!(findings("(+ 1 2 (* 3 4))").is_empty());
        assert!(findings("(floor total count)").is_empty());
    }

    /// `incf`'s first argument is a place, not a value, so it is out of scope.
    #[test]
    fn does_not_flag_incf_or_decf() {
        assert!(findings("(incf total (when p 1))").is_empty());
        assert!(findings("(decf total (when p 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_argument() {
        assert!(findings("(+ 1 '(when p 2))").is_empty());
        assert!(findings("(+ 1 #'(when p 2))").is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_argument_list() {
        assert!(findings("(+ base #+sbcl (when p 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_symbol_named_when() {
        assert!(findings("(+ base when)").is_empty());
    }

    // -- the five quote shapes -----------------------------------------------

    const CANDIDATE: &str = "(+ base (when p d))";

    #[test]
    fn bare_code_fires() {
        assert_eq!(findings(CANDIDATE).len(), 1);
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(findings(&format!("'{CANDIDATE}")).is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(findings(&format!("(quote {CANDIDATE})")).is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(findings(&format!("'(a ,{CANDIDATE})")).is_empty());
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert_eq!(findings(&format!("`(a ,{CANDIDATE})")).len(), 1);
    }

    #[test]
    fn an_arithmetic_form_inside_a_string_literal_is_not_a_form() {
        assert!(findings("(format t \"(+ base (when p d))\")").is_empty());
    }

    // -- envelope ------------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CANDIDATE, Dialect::Clojure).expect("parse");
        let built = build_when_unless_implicit_nil_misused_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_strict_form_scanned_not_only_the_flagged_ones() {
        let built = report(&format!("{CANDIDATE}\n(* 2 3)\n"));
        assert_eq!(built.summary, vec![("arithmetic_form_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_fields() {
        let built = report(&format!("(defun f (base p d)\n  {CANDIDATE})\n"));
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("+")),
                ("conditional", json!("when")),
                ("argument_index", json!(2_usize)),
            ]
        );
    }
}
