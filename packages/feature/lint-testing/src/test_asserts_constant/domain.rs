//! `test-asserts-constant` detection: an assertion whose truth is settled by
//! the source, so it passes no matter what the code under test does.
//!
//! # What this attempts
//!
//! Two shapes, both inside a readable test definition, and both restricted to
//! the *unary* assertion macros (`is`, `is-true`, `assert-true`, `should`):
//!
//! 1. **A literal truth**: `(is t)`, `(should t)`, `(is true)`. The assertion's
//!    only argument is the dialect's own spelling of true.
//! 2. **A literal self-equality**: `(is (= 1 1))`, `(should (equal "a" "a"))`.
//!    The assertion's only argument compares two *literals* whose source text
//!    is identical.
//!
//! # What this does not attempt
//!
//! - **`(= x x)` on symbols.** Two identical symbols may still be a genuine
//!   check (of `equalp` on a mutable place, of a float that is `NaN`), and in
//!   Common Lisp `self-comparison` already reports that shape. Only literals
//!   count here.
//! - **The equality heads `self-comparison` already owns.** That rule is Common
//!   Lisp only and lists `eq`, `eql`, `equal`, `equalp`, `string=`, `char=` and
//!   the six ordering operators — so in Common Lisp this rule claims only `=`,
//!   which is the one equality head absent from its list. Reporting the same
//!   `(is (equal 1 1))` twice under two rule names would be worse than
//!   reporting it once.
//! - **An always-*failing* assertion.** `(is nil)` never passes, which is a
//!   different defect and, notably, is exactly what this repository's own
//!   `generate tests` scaffolding emits as a deliberate placeholder.
//! - **`cl:assert` / `cl-assert`.** General defensive assertions, not test
//!   framework ones; see [`crate::support::boolean_assertion_heads`].
//! - **The n-ary assertions.** `(assert-equal 3 3)` takes an expected value and
//!   a form, so a literal first argument is the normal case.
//!
//! # A known tension with `test-without-assertion`
//!
//! A test whose intent is "this call does not throw" has no assertion to make,
//! and the only assertion that expresses that intent is `(is t)` — which this
//! rule then flags, so the two rules can in principle disagree about the same
//! test. In practice they do not close the loop, because `test-without-assertion`
//! goes quiet as soon as a body calls anything it does not recognize, which
//! such a test always does; the tension is recorded here rather than resolved,
//! and neither rule's behaviour is conditioned on the other's.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    TEST_DIALECTS, boolean_assertion_heads, calls_any, for_each_evaluated_subview,
    is_false_literal, is_string_literal, is_true_literal, read_test_form,
};

/// Which of the two shapes a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstantShape {
    /// `(is t)` — the argument is the dialect's literal for truth.
    LiteralTruth,
    /// `(is (= 1 1))` — the argument compares two identical literals.
    LiteralSelfEquality,
}

impl ConstantShape {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LiteralTruth => "literal-truth",
            Self::LiteralSelfEquality => "literal-self-equality",
        }
    }

    pub(crate) const fn detail(self) -> &'static str {
        match self {
            Self::LiteralTruth => "its argument is the literal true",
            Self::LiteralSelfEquality => "it compares a literal with itself",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestAssertsConstantItem {
    /// The span of the assertion call.
    pub span: ByteSpan,
    /// The enclosing test's name.
    pub test_name: String,
    /// The assertion macro as written.
    pub assertion: String,
    /// Which tautology this is.
    pub shape: ConstantShape,
}

impl Finding for TestAssertsConstantItem {
    fn kind(&self) -> &'static str {
        "test-asserts-constant"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("test={}", self.test_name),
            format!("shape={}", self.shape.as_str()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("test", json!(self.test_name)),
            ("assertion", json!(self.assertion)),
            ("shape", json!(self.shape.as_str())),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} in test {} can never fail: {}",
            self.assertion,
            self.test_name,
            self.shape.detail()
        )
    }
}

/// The equality heads this rule may claim, per dialect.
///
/// Common Lisp gets `=` alone because `self-comparison` — which is Common Lisp
/// only — already reports the other fourteen. Emacs Lisp and Clojure are
/// outside that rule's scope entirely, so their equality heads are unclaimed.
const fn self_equality_heads(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["="],
        Dialect::EmacsLisp => &["=", "eq", "eql", "equal", "string=", "string-equal"],
        Dialect::Clojure => &["=", "=="],
        _ => &[],
    }
}

/// Whether `text` is a numeric literal.
///
/// Written here rather than reused because `core/syntax`'s equivalent is
/// `pub(crate)`. Deliberately stricter than `f64::from_str`, which accepts
/// `inf` and `NaN` — both of which are ordinary symbols in every dialect here,
/// and neither of which may be read as a literal.
fn is_number_literal(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if !body.starts_with(|first: char| first.is_ascii_digit() || first == '.') {
        return false;
    }
    // A Common Lisp ratio, which `f64::from_str` does not accept.
    if let Some((numerator, denominator)) = body.split_once('/') {
        return !numerator.is_empty()
            && !denominator.is_empty()
            && numerator.bytes().all(|byte| byte.is_ascii_digit())
            && denominator.bytes().all(|byte| byte.is_ascii_digit());
    }
    body.parse::<f64>().is_ok()
}

/// Whether an atom denotes itself: a number, a string, a character, a keyword,
/// or one of the dialect's boolean/nil literals.
///
/// A bare symbol is deliberately *not* self-evaluating here. `(= x x)` may be a
/// real check and is another rule's subject; only text whose value is fixed by
/// the source counts.
fn is_self_evaluating(view: &ExpressionView, dialect: Dialect) -> bool {
    if is_true_literal(view, dialect) || is_false_literal(view, dialect) {
        return true;
    }
    let Some(text) = atom_symbol_text(view) else {
        return false;
    };
    if text.is_empty() {
        return false;
    }
    is_string_literal(view)
        || is_number_literal(text)
        || text.starts_with(':')
        || text.starts_with("#\\")
        || text.starts_with('\\')
}

/// Whether two nodes are the same literal written the same way.
///
/// Compared by exact source text, not by value: `1` and `1.0` are different
/// literals here, and a rule that guessed otherwise would be claiming to know
/// a reader's numeric tower.
fn same_literal(left: &ExpressionView, right: &ExpressionView, dialect: Dialect) -> bool {
    if !is_self_evaluating(left, dialect) || !is_self_evaluating(right, dialect) {
        return false;
    }
    match (atom_text(left), atom_text(right)) {
        (Some(left_text), Some(right_text)) => left_text == right_text,
        _ => false,
    }
}

/// Which tautology `argument` is, if any.
fn constant_shape(argument: &ExpressionView, dialect: Dialect) -> Option<ConstantShape> {
    if is_true_literal(argument, dialect) {
        return Some(ConstantShape::LiteralTruth);
    }
    if !calls_any(argument, self_equality_heads(dialect)) {
        return None;
    }
    // Exactly two operands: `(= 1 1 1)` is a shape this rule does not model.
    let [_, left, right] = argument.children.as_slice() else {
        return None;
    };
    same_literal(left, right, dialect).then_some(ConstantShape::LiteralSelfEquality)
}

/// Examines one node. The caller guarantees `view` is evaluated code.
pub fn examine_test(
    view: &ExpressionView,
    dialect: Dialect,
    assertion_form_count: &mut usize,
    violations: &mut Vec<TestAssertsConstantItem>,
) {
    let Some(form) = read_test_form(view, dialect) else {
        return;
    };
    let Some(test_name) = form.name_text() else {
        return;
    };

    let heads = boolean_assertion_heads(dialect);
    for body_form in form.body {
        for_each_evaluated_subview(body_form, |candidate| {
            let Some(head) = list_head(candidate) else {
                return;
            };
            if !symbol_in(head, heads) {
                return;
            }
            *assertion_form_count += 1;
            // Unary only: a second argument to `is` is FiveAM's failure
            // description, which does not change the verdict, but a `should`
            // with two arguments is not a shape this rule models.
            let Some(argument) = candidate.children.get(1) else {
                return;
            };
            let Some(shape) = constant_shape(argument, dialect) else {
                return;
            };
            violations.push(TestAssertsConstantItem {
                span: candidate.span,
                test_name: test_name.clone(),
                assertion: head.to_owned(),
                shape,
            });
        });
    }
}

/// Collects every tautological assertion in one file, with the number of unary
/// assertions scanned beside them.
pub fn build_test_asserts_constant_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TestAssertsConstantItem>> {
    let modelled = TEST_DIALECTS.contains(&dialect);
    let mut assertion_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            if list_head(view).is_some() {
                examine_test(view, dialect, &mut assertion_form_count, &mut violations);
            }
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("assertion_form_count", json!(assertion_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(input: &str, dialect: Dialect) -> Vec<TestAssertsConstantItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_test_asserts_constant_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
            .findings
    }

    fn shapes(input: &str, dialect: Dialect) -> Vec<ConstantShape> {
        findings(input, dialect)
            .into_iter()
            .map(|item| item.shape)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_literal_truth_in_each_modelled_dialect() {
        assert_eq!(
            shapes("(def-test adds () (is t))", Dialect::CommonLisp),
            vec![ConstantShape::LiteralTruth]
        );
        assert_eq!(
            shapes("(ert-deftest adds () (should t))", Dialect::EmacsLisp),
            vec![ConstantShape::LiteralTruth]
        );
        assert_eq!(
            shapes("(deftest adds (is true))", Dialect::Clojure),
            vec![ConstantShape::LiteralTruth]
        );
    }

    #[test]
    fn flags_an_is_true_on_a_literal() {
        assert_eq!(
            shapes("(def-test adds () (is-true t))", Dialect::CommonLisp),
            vec![ConstantShape::LiteralTruth]
        );
        assert_eq!(
            shapes("(define-test adds (assert-true t))", Dialect::CommonLisp),
            vec![ConstantShape::LiteralTruth]
        );
    }

    #[test]
    fn flags_a_literal_compared_with_itself() {
        assert_eq!(
            shapes("(deftest adds (is (= 1 1)))", Dialect::Clojure),
            vec![ConstantShape::LiteralSelfEquality]
        );
        assert_eq!(
            shapes("(def-test adds () (is (= 1 1)))", Dialect::CommonLisp),
            vec![ConstantShape::LiteralSelfEquality]
        );
        assert_eq!(
            shapes(
                "(ert-deftest adds () (should (equal \"a\" \"a\")))",
                Dialect::EmacsLisp
            ),
            vec![ConstantShape::LiteralSelfEquality]
        );
    }

    #[test]
    fn flags_an_assertion_nested_below_a_grouping_form() {
        assert_eq!(
            shapes(
                "(deftest adds (testing \"sums\" (is true)))",
                Dialect::Clojure
            ),
            vec![ConstantShape::LiteralTruth]
        );
    }

    // -- near misses ---------------------------------------------------------

    #[test]
    fn an_assertion_on_a_real_expression_is_silent() {
        assert!(shapes("(deftest adds (is (= 3 (+ 1 2))))", Dialect::Clojure).is_empty());
        assert!(
            shapes(
                "(ert-deftest adds () (should (my-p x)))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
        assert!(shapes("(def-test adds () (is (evenp 2)))", Dialect::CommonLisp).is_empty());
    }

    /// Always-false is a different defect — and is what this repository's own
    /// `generate tests` scaffolding writes on purpose.
    #[test]
    fn an_always_failing_assertion_is_not_this_rules_subject() {
        assert!(shapes("(deftest adds (is nil))", Dialect::Clojure).is_empty());
        assert!(shapes("(deftest adds (is false))", Dialect::Clojure).is_empty());
        assert!(shapes("(def-test adds () (is nil))", Dialect::CommonLisp).is_empty());
        assert!(shapes("(deftest adds (is (= 1 2)))", Dialect::Clojure).is_empty());
    }

    /// Two identical *symbols* are not two identical literals.
    #[test]
    fn a_self_comparison_of_symbols_is_left_alone() {
        assert!(shapes("(deftest adds (is (= x x)))", Dialect::Clojure).is_empty());
        assert!(shapes("(def-test adds () (is (= x x)))", Dialect::CommonLisp).is_empty());
    }

    /// The carve-out that keeps this rule from double-reporting with
    /// `self-comparison`, which is Common Lisp only and owns these heads.
    #[test]
    fn common_lisp_equality_heads_owned_by_self_comparison_are_not_claimed() {
        assert!(shapes("(def-test adds () (is (equal 1 1)))", Dialect::CommonLisp).is_empty());
        assert!(shapes("(def-test adds () (is (eql 1 1)))", Dialect::CommonLisp).is_empty());
        assert!(
            shapes(
                "(def-test adds () (is (string= \"a\" \"a\")))",
                Dialect::CommonLisp
            )
            .is_empty()
        );
        // The same head in Emacs Lisp is outside `self-comparison`'s scope, so
        // it *is* claimed here.
        assert_eq!(
            shapes(
                "(ert-deftest adds () (should (eql 1 1)))",
                Dialect::EmacsLisp
            ),
            vec![ConstantShape::LiteralSelfEquality]
        );
    }

    #[test]
    fn different_literals_are_not_a_tautology() {
        assert!(shapes("(deftest adds (is (= 1 1.0)))", Dialect::Clojure).is_empty());
        assert!(shapes("(deftest adds (is (= \"a\" \"b\")))", Dialect::Clojure).is_empty());
    }

    #[test]
    fn an_n_ary_equality_is_not_modelled() {
        assert!(shapes("(deftest adds (is (= 1 1 1)))", Dialect::Clojure).is_empty());
    }

    #[test]
    fn the_general_runtime_assertions_are_not_test_assertions() {
        assert!(shapes("(def-test adds () (assert t))", Dialect::CommonLisp).is_empty());
        assert!(shapes("(ert-deftest adds () (cl-assert t))", Dialect::EmacsLisp).is_empty());
    }

    #[test]
    fn an_expected_value_that_happens_to_be_a_literal_is_not_a_tautology() {
        assert!(shapes("(define-test adds (assert-equal 3 3))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn an_assertion_outside_any_test_is_not_this_rules_subject() {
        assert!(shapes("(defn check [] (is true))", Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_true_spelling_from_another_dialect_is_not_true() {
        // `t` is an ordinary symbol in Clojure.
        assert!(shapes("(deftest adds (is t))", Dialect::Clojure).is_empty());
        // `true` is an ordinary symbol in Common Lisp.
        assert!(shapes("(def-test adds () (is true))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_test_using_an_unmodelled_framework_is_silent() {
        assert!(shapes("(fact \"adds\" (is true))", Dialect::Clojure).is_empty());
    }

    // -- quote and string negatives ------------------------------------------

    #[test]
    fn a_quoted_test_form_is_data_and_is_not_flagged() {
        assert!(shapes("'(deftest adds (is true))", Dialect::Clojure).is_empty());
        assert!(shapes("(quote (deftest adds (is true)))", Dialect::Clojure).is_empty());
    }

    /// The assertion is data even though the test around it is code.
    #[test]
    fn an_assertion_inside_quoted_data_is_not_an_assertion() {
        assert!(shapes("(deftest adds (is (= '(is true) form)))", Dialect::Clojure).is_empty());
    }

    /// Written in Common Lisp on purpose: `,` is an unquote there and plain
    /// whitespace in Clojure.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_make_an_assertion_code_again() {
        assert!(shapes("'(a ,(def-test adds () (is t)))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_quasiquoted_macro_template_is_data() {
        assert!(shapes("(defmacro m [n] `(deftest ~n (is true)))", Dialect::Clojure).is_empty());
    }

    /// Clojure spells unquote `~`; a `,` there is whitespace.
    #[test]
    fn an_unquoted_assertion_inside_a_quasiquote_is_code() {
        assert_eq!(
            shapes("`(a ~(deftest adds (is true)))", Dialect::Clojure),
            vec![ConstantShape::LiteralTruth]
        );
        assert_eq!(
            shapes("`(a ,(def-test adds () (is t)))", Dialect::CommonLisp),
            vec![ConstantShape::LiteralTruth]
        );
    }

    #[test]
    fn an_assertion_spelled_inside_a_string_is_not_an_assertion() {
        assert!(shapes("(deftest adds (is (= \"(is true)\" s)))", Dialect::Clojure).is_empty());
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn an_unmodelled_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(deftest adds (is #t))", Dialect::Scheme)
            .expect("parse");
        let report = build_test_asserts_constant_report(Path::new("a.scm"), Dialect::Scheme, &tree)
            .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_unary_assertion_scanned() {
        let tree = SyntaxTree::parse_with_dialect(
            "(deftest a (is true) (is (= 3 (+ 1 2))) (is (= 1 1)))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_test_asserts_constant_report(Path::new("t.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert_eq!(report.summary, vec![("assertion_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn a_finding_names_its_test_its_assertion_and_its_shape() {
        let tree = SyntaxTree::parse_with_dialect(
            "(ns app)\n(deftest adds\n  (is true))\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_test_asserts_constant_report(Path::new("t.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "test-asserts-constant");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("test", json!("adds")),
                ("assertion", json!("is")),
                ("shape", json!("literal-truth")),
            ]
        );
    }
}
