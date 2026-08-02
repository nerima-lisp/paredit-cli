//! Common Lisp `case`-key detection for the two literal kinds `eql` does not
//! match dependably: **strings** and **floats**.
//!
//! `case` dispatches with `eql`. CLHS does not say so in those words on the
//! `case` page, but it gives the expansion as
//! `(let ((g test-key)) (cond ((member g '(key…)) form…) …))`, and `member`'s
//! default `:test` is `eql`. So a `case` key is only ever as good as `eql` is
//! on it.
//!
//! # What CLHS actually guarantees, and where the pitfall really is
//!
//! `eql` is true of two objects when they are `eq`, when they are "both
//! numbers of the same type and the same value", or when they are characters
//! representing the same character. Read carefully, that clause is a *promise*
//! for most literals and the rule below reports none of them:
//!
//! - **Bignums are fine.** Two bignums of the same value are numbers of the
//!   same type and the same value, so `eql` is true of them. There is no
//!   bignum pitfall to report, and a rule that claimed one would be wrong.
//! - **Integers, ratios, characters and keywords are fine**, by the same
//!   clause and the character clause.
//!
//! The two that are genuinely not fine:
//!
//! - **Strings.** A string is not a number or a character, so `eql` falls
//!   through to `eq` — object identity. CLHS's own examples give
//!   `(eql "Foo" "Foo")` as *true or false* depending on the implementation,
//!   and `(eql "Foo" (copy-seq "Foo"))` as false. A `case` clause keyed on a
//!   string therefore matches at the implementation's discretion at best, and
//!   never for a string the program computed or read. `string=` in a `cond`, or
//!   a `trivia`-style matcher, is what that code wants.
//! - **Floats.** Not because `eql` is unreliable on a float — it is not, two
//!   floats of the same type and value are `eql` — but because the *type* a
//!   float literal reads as is decided by `*read-default-float-format*`, whose
//!   standard default is `single-float`. CLHS notes that "normally
//!   `(eql 1.0s0 1.0d0)` is false, under the assumption that `1.0s0` and
//!   `1.0d0` are of distinct data types". So `(case x (1.0 …))` silently fails
//!   to match a double-float `x`, and whether it matches at all depends on
//!   reader state rather than on the code. Rounding upstream makes an exact
//!   float key worse still.
//!
//! # Disjointness
//!
//! - A *quoted* key (`'a`, `(quote a)`) is `quoted-case-key`'s subject and is
//!   skipped here, including a quoted string.
//! - A bare `nil` key is `case-nil-key`'s, and a repeated key is
//!   `duplicate-case-keys`'; neither classifies a key's literal type.
//! - `eql-string-comparison` and `float-equality` report the same two mistakes
//!   spelt as *calls* (`(eql x "s")`, `(= x 1.0)`); their head filters are the
//!   comparison operators and never `case`, so no node is reported twice.
//! - `typecase` and its variants are excluded: their clause heads are type
//!   specifiers, matched with `typep`, not keys matched with `eql`.
//!
//! Report-only: the repair is a different operator (`string=`, an epsilon
//! comparison), not a rewrite of the key, and choosing it is the author's.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    LiteralKind, for_each_evaluated_subview, has_reader_conditional_child, is_clause, literal_kind,
};

/// The `eql`-dispatching case forms. `typecase` and friends dispatch with
/// `typep` and are deliberately absent.
const CASE_HEADS: [&str; 3] = ["case", "ecase", "ccase"];

/// Which of the two undependable literal kinds a key is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitfallKind {
    /// `eql` on two separately-read strings is `eq`, i.e. implementation's
    /// discretion at best.
    StringKey,
    /// The literal's float *type* comes from `*read-default-float-format*`.
    FloatKey,
}

impl PitfallKind {
    /// The finding's discriminator. Two distinct shapes, so the rule name alone
    /// would not say which was found.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StringKey => "case-key-string",
            Self::FloatKey => "case-key-float",
        }
    }

    /// The clause of CLHS the complaint rests on.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::StringKey => {
                "case matches keys with eql, and eql on two separately-read strings is eq"
            }
            Self::FloatKey => {
                "case matches keys with eql, and a float literal's type comes from \
                 *read-default-float-format*"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CaseKeyEqlPitfallItem {
    /// The span of the offending key itself, not of the clause or the form.
    pub span: ByteSpan,
    /// The key's source text.
    pub key: String,
    pub pitfall: PitfallKind,
}

impl Finding for CaseKeyEqlPitfallItem {
    fn kind(&self) -> &'static str {
        self.pitfall.as_str()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.key.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("key", json!(self.key))]
    }

    fn message(&self) -> String {
        format!(
            "{} never matches dependably: {}",
            self.key,
            self.pitfall.reason()
        )
    }
}

/// The pitfall a single key carries, if it carries one.
fn key_pitfall(key: &ExpressionView) -> Option<PitfallKind> {
    match literal_kind(key) {
        LiteralKind::String => Some(PitfallKind::StringKey),
        LiteralKind::Float => Some(PitfallKind::FloatKey),
        _ => None,
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_case(
    view: &ExpressionView,
    case_form_count: &mut usize,
    violations: &mut Vec<CaseKeyEqlPitfallItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &CASE_HEADS) {
        return;
    }
    *case_form_count += 1;

    // `(case test-key clause*)`: the clauses start after the test key.
    let Some(clauses) = view.children.get(2..) else {
        return;
    };
    for clause in clauses {
        if !is_clause(clause) || has_reader_conditional_child(clause) {
            continue;
        }
        let Some(designator) = clause.children.first() else {
            continue;
        };
        if is_paren_list(designator) {
            // A key *list*: every element is its own key.
            for key in &designator.children {
                if let Some(pitfall) = key_pitfall(key) {
                    violations.push(CaseKeyEqlPitfallItem {
                        span: key.span,
                        key: key.text.clone().unwrap_or_default(),
                        pitfall,
                    });
                }
            }
            continue;
        }
        // No explicit `t`/`otherwise` guard: both are symbols, and
        // `key_pitfall` answers `None` for every kind but a string and a
        // float. A guard here was written and then removed after mutation
        // testing showed no test could tell whether it was present.
        if let Some(pitfall) = key_pitfall(designator) {
            violations.push(CaseKeyEqlPitfallItem {
                span: designator.span,
                key: designator.text.clone().unwrap_or_default(),
                pitfall,
            });
        }
    }
}

/// Collects every string- or float-keyed `case` clause in one file, with the
/// number of `case` forms scanned as the denominator beside them.
pub fn build_case_key_eql_pitfall_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CaseKeyEqlPitfallItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("case_form_count", json!(0))],
        ));
    }

    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine_case(subview, &mut case_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("case_form_count", json!(case_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CaseKeyEqlPitfallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_case_key_eql_pitfall_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build case-key-eql-pitfall report")
    }

    fn findings(input: &str) -> Vec<CaseKeyEqlPitfallItem> {
        report(input).findings
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_string_key() {
        let items = findings(r#"(case cmd ("add" 1) (:sub 2))"#);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].pitfall, PitfallKind::StringKey);
        assert_eq!(items[0].key, "\"add\"");
        assert_eq!(items[0].kind(), "case-key-string");
    }

    #[test]
    fn flags_a_float_key() {
        let items = findings("(case x (1.0 :one) (2 :two))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].pitfall, PitfallKind::FloatKey);
        assert_eq!(items[0].key, "1.0");
        assert_eq!(items[0].kind(), "case-key-float");
    }

    #[test]
    fn flags_a_pitfall_key_inside_a_key_list() {
        let items = findings(r#"(case cmd ((:a "b" 1) :hit) (t :miss))"#);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "\"b\"");
    }

    #[test]
    fn flags_ecase_and_ccase_too() {
        assert_eq!(findings(r#"(ecase c ("a" 1))"#).len(), 1);
        assert_eq!(findings("(ccase c (1.5 1))").len(), 1);
    }

    #[test]
    fn flags_every_offending_key_not_only_the_first() {
        let items = findings(r#"(case c ("a" 1) ("b" 2) (3.5 3))"#);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        assert_eq!(findings(r#"(defun f (c) (case c ("a" 1)))"#).len(), 1);
    }

    #[test]
    fn every_float_spelling_is_flagged() {
        for key in ["1.0", ".5", "1e5", "1.0d0", "-2.5"] {
            assert_eq!(
                findings(&format!("(case x ({key} :hit))")).len(),
                1,
                "{key}"
            );
        }
    }

    // -- near-miss negatives: what CLHS actually guarantees -------------------

    /// The premise this rule was almost built on. CLHS: `eql` is true of two
    /// objects that are "both numbers of the same type and the same value", so
    /// two bignums of the same value *are* `eql`. There is no pitfall here.
    #[test]
    fn does_not_flag_a_bignum_key() {
        assert!(findings("(case x (100000000000000000000 :big) (t :other))").is_empty());
    }

    #[test]
    fn does_not_flag_integer_ratio_character_or_keyword_keys() {
        assert!(findings("(case x (1 :a) (2 :b))").is_empty());
        assert!(findings("(case x (1/2 :a))").is_empty());
        assert!(findings("(case x (#\\a :a))").is_empty());
        assert!(findings("(case x (:a 1) (:b 2))").is_empty());
    }

    /// `1.` is the integer 1 (CLHS 2.3.1), not a float.
    #[test]
    fn does_not_flag_a_decimal_point_terminated_integer() {
        assert!(findings("(case x (1. :one))").is_empty());
    }

    #[test]
    fn does_not_flag_a_symbol_key() {
        assert!(findings("(case x (foo 1) (bar 2))").is_empty());
    }

    #[test]
    fn does_not_flag_the_catch_all_designators() {
        assert!(findings("(case x (1 :a) (t :other))").is_empty());
        assert!(findings("(case x (1 :a) (otherwise :other))").is_empty());
    }

    /// A quoted key is `quoted-case-key`'s subject, so this stays silent even
    /// when the quoted datum is a string.
    #[test]
    fn does_not_flag_a_quoted_key() {
        assert!(findings("(case x ('a 1))").is_empty());
        assert!(findings(r#"(case x ('"a" 1))"#).is_empty());
        assert!(findings("(case x ((quote a) 1))").is_empty());
    }

    /// `typecase` heads are type specifiers matched with `typep`, not keys.
    #[test]
    fn does_not_flag_a_typecase_form() {
        assert!(findings("(typecase x (string 1) (float 2))").is_empty());
        assert!(findings("(etypecase x (string 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_clause() {
        assert!(findings(r#"(case x (#+sbcl "a" 1))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_clause() {
        // A bare atom in clause position is `malformed-case-clause`'s subject.
        assert!(findings(r#"(case x "a")"#).is_empty());
    }

    /// A string in the *body* is an ordinary value, not a key.
    #[test]
    fn does_not_flag_a_string_in_a_clause_body() {
        assert!(findings(r#"(case x (1 "one") (2 "two"))"#).is_empty());
    }

    /// The test-key expression is not a key either.
    #[test]
    fn does_not_flag_the_test_key_position() {
        assert!(findings(r#"(case "literal" (1 :a))"#).is_empty());
    }

    // -- the five quote shapes -----------------------------------------------

    const CANDIDATE: &str = r#"(case c ("a" 1) (t 2))"#;

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
    fn a_case_inside_a_string_literal_is_not_a_form() {
        assert!(findings("(format t \"(case c (1 2))\")").is_empty());
    }

    // -- envelope ------------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CANDIDATE, Dialect::Clojure).expect("parse");
        let built = build_case_key_eql_pitfall_report(Path::new("a.clj"), Dialect::Clojure, &tree)
            .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_case_scanned_not_only_the_flagged_ones() {
        let built = report(&format!("{CANDIDATE}\n(case n (1 :a) (2 :b))\n"));
        assert_eq!(built.summary, vec![("case_form_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_fields() {
        let built = report(&format!("(defun f (c)\n  {CANDIDATE})\n"));
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(finding.json_fields(), vec![("key", json!("\"a\""))]);
    }
}
