//! Common Lisp cond-to-`case` detection: a `cond` whose every clause tests one
//! and the same variable against a literal with `eq`/`eql`/`equal`, which
//! `case` says in one line per key instead of one call per key.
//!
//! `(cond ((eql op 1) :add) ((eql op 2) :sub) (t :other))` dispatches on `op`
//! and nothing else, which is what `case` is for. The `cond` spelling repeats
//! both the operator and the variable on every line, so a mistyped variable in
//! the fifth clause reads exactly like the other four.
//!
//! # What this refuses to report, and why
//!
//! `cond` evaluates arbitrary tests and `case` compares with `eql` (CLHS gives
//! `case`'s expansion as `(cond ((member key '(keys)) …))`, and `member`'s
//! default test is `eql`). The two are therefore *not* interchangeable in
//! general, and every guard below exists to keep a reported `cond` one that a
//! `case` would genuinely reproduce:
//!
//! - **One variable, named the same way every time.** Every test must compare
//!   the same bare symbol. A bare symbol has no side effect and yields the same
//!   value each time it is read, so hoisting it to `case`'s single test-key
//!   position changes nothing. A compound test operand — `(eql (pop s) 1)` —
//!   would be evaluated once by `case` and once per clause by `cond`, so it is
//!   never reported.
//! - **Keys `eql` matches dependably.** A string or float key is excluded:
//!   `case` would compare it with `eql`, which CLHS does not promise is true of
//!   two separately-read strings, and whose answer for a float literal depends
//!   on what type `*read-default-float-format*` gave it. `(equal x "s")` is
//!   emphatically **not** a `case` clause, and this is the guard that says so.
//! - **No `t` or `nil` key.** Those are `case`'s catch-all designators, not
//!   keys; `(case x (nil …))` is `case-nil-key`'s subject, not a conversion.
//! - **Every clause has a body.** `(cond ((eql x 1)))` returns the *test's*
//!   value `t`, while `(case x (1))` returns `nil`. Converting a body-less
//!   clause would change what the form evaluates to.
//! - **A catch-all only in final position**, and only the literal `t` —
//!   `otherwise` is not a `cond` catch-all, it is an ordinary variable
//!   reference.
//! - **Three comparison clauses at least.** Two is a `case` too, but reporting
//!   it turns every small `cond` into a finding; the threshold buys a large
//!   drop in noise for a false negative nobody misses.
//!
//! Reader-conditional clauses (`#+sbcl`) have no settled shape and are left
//! alone, as everywhere else in this package.
//!
//! Report-only on purpose. Even a `cond` that passes every guard above is
//! *equivalent* to a `case` rather than *better* than one, and this project has
//! a documented history of autofixes silently corrupting source; rewriting
//! control flow to chase a style preference is not a trade worth making.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    LiteralKind, bare_symbol, for_each_evaluated_subview, has_reader_conditional_child, is_clause,
    literal_kind,
};

/// The comparison operators a `case` key reproduces.
///
/// `equal` is included because on every key kind this rule accepts — integers,
/// ratios, characters, keywords, quoted symbols — `equal` is defined to fall
/// back on `eql`. It is *not* included for strings, and that is precisely what
/// [`LiteralKind::is_eql_dependable`] rules out.
const CONVERTIBLE_COMPARATORS: [&str; 3] = ["eq", "eql", "equal"];

/// The fewest comparison clauses this reports on. See the module docs.
const MIN_COMPARISON_CLAUSES: usize = 3;

#[derive(Debug, Clone)]
pub struct CondToCaseItem {
    /// The span of the whole `cond` form.
    pub span: ByteSpan,
    /// The single variable every clause tests, in its normalized spelling.
    pub variable: String,
    /// How many comparison clauses were found, excluding any final `t`.
    pub comparison_clauses: usize,
}

impl Finding for CondToCaseItem {
    fn kind(&self) -> &'static str {
        "cond-to-case-candidate"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.variable.clone(), self.comparison_clauses.to_string()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("variable", json!(self.variable)),
            ("comparison_clauses", json!(self.comparison_clauses)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "every cond test compares {} against a literal; case says this directly",
            self.variable
        )
    }
}

/// The variable a single `(eq|eql|equal …)` test dispatches on, when the test
/// is one this rule can convert.
///
/// Accepts both operand orders — `(eql x 1)` and `(eql 1 x)` — because both
/// spell the same comparison and both become the same `case` key.
fn dispatch_variable(test: &ExpressionView) -> Option<String> {
    let head = list_head(test)?;
    if !symbol_in(head, &CONVERTIBLE_COMPARATORS) {
        return None;
    }
    // Exactly the operator and two operands. A three-operand `eql` is an arity
    // defect, which is a different rule's subject.
    if test.children.len() != 3 {
        return None;
    }
    if has_reader_conditional_child(test) {
        return None;
    }

    let left = &test.children[1];
    let right = &test.children[2];
    // Exactly one side is the variable and the other the key. The two roles are
    // read by *kind* and their kinds are disjoint, so the pairing is
    // unambiguous and `(eql a b)` — two variables, no literal — is rejected.
    match (
        dispatched_variable_name(left),
        dispatched_variable_name(right),
    ) {
        (Some(name), None) if is_convertible_key(right) => Some(name),
        (None, Some(name)) if is_convertible_key(left) => Some(name),
        _ => None,
    }
}

/// The name of an operand that is a plain variable reference.
///
/// `t` and `nil` are symbols but name constants, not variables, so neither can
/// be the value a `case` dispatches on.
fn dispatched_variable_name(operand: &ExpressionView) -> Option<String> {
    bare_symbol(operand).filter(|name| !is_catch_all_name(name))
}

/// `t` and `nil` name `case`'s designators rather than a value, at both the
/// variable and the key position.
fn is_catch_all_name(name: &str) -> bool {
    name == "t" || name == "nil"
}

/// Whether `key` is a literal a `case` clause can carry unchanged.
///
/// A bare symbol is deliberately **not** convertible: in a `cond` test an
/// unquoted symbol is a variable reference, and the symbol-valued key is spelt
/// `'foo`, which reads as [`LiteralKind::QuotedSymbol`]. Strings and floats are
/// excluded by [`LiteralKind::is_eql_dependable`].
fn is_convertible_key(key: &ExpressionView) -> bool {
    matches!(
        literal_kind(key),
        LiteralKind::Integer
            | LiteralKind::Ratio
            | LiteralKind::Character
            | LiteralKind::Keyword
            | LiteralKind::QuotedSymbol
    )
}

/// Whether `clause` is the literal-`t` catch-all. `otherwise` is deliberately
/// not one: in `cond` it is an ordinary variable reference.
fn is_catch_all_clause(clause: &ExpressionView) -> bool {
    clause
        .children
        .first()
        .and_then(bare_symbol)
        .is_some_and(|name| name == "t")
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_cond(
    view: &ExpressionView,
    cond_form_count: &mut usize,
    violations: &mut Vec<CondToCaseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_is(head, "cond") {
        return;
    }
    *cond_form_count += 1;

    let clauses = &view.children[1..];
    if clauses.len() < MIN_COMPARISON_CLAUSES {
        return;
    }

    let mut variable: Option<String> = None;
    let mut comparison_clauses = 0;

    for (index, clause) in clauses.iter().enumerate() {
        if !is_clause(clause) || has_reader_conditional_child(clause) {
            return;
        }
        // Every clause needs a body: a test-only clause returns the *test's*
        // value, which the converted `case` clause would not.
        if clause.children.len() < 2 {
            return;
        }
        let is_last = index + 1 == clauses.len();
        if is_last && is_catch_all_clause(clause) {
            break;
        }
        let Some(name) = dispatch_variable(&clause.children[0]) else {
            return;
        };
        match &variable {
            Some(seen) if *seen != name => return,
            Some(_) => {}
            None => variable = Some(name),
        }
        comparison_clauses += 1;
    }

    if comparison_clauses < MIN_COMPARISON_CLAUSES {
        return;
    }
    let Some(variable) = variable else {
        return;
    };

    violations.push(CondToCaseItem {
        span: view.span,
        variable,
        comparison_clauses,
    });
}

/// Collects every `cond` that dispatches on one variable in one file, with the
/// number of `cond` forms scanned as the denominator beside them.
pub fn build_cond_to_case_candidate_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CondToCaseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("cond_form_count", json!(0))],
        ));
    }

    let mut cond_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine_cond(subview, &mut cond_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("cond_form_count", json!(cond_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CondToCaseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_cond_to_case_candidate_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build cond-to-case-candidate report")
    }

    fn findings(input: &str) -> Vec<CondToCaseItem> {
        report(input).findings
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_cond_that_dispatches_on_one_variable() {
        let items = findings("(cond ((eql op 1) :add) ((eql op 2) :sub) ((eql op 3) :mul))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, "op");
        assert_eq!(items[0].comparison_clauses, 3);
    }

    #[test]
    fn a_final_t_clause_is_the_else_and_does_not_block_the_report() {
        let items =
            findings("(cond ((eql op 1) :add) ((eql op 2) :sub) ((eql op 3) :mul) (t :other))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].comparison_clauses, 3);
    }

    #[test]
    fn both_operand_orders_are_the_same_comparison() {
        let items = findings("(cond ((eql 1 op) :add) ((eql op 2) :sub) ((eql 3 op) :mul))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, "op");
    }

    #[test]
    fn keyword_and_character_and_quoted_symbol_keys_are_all_convertible() {
        assert_eq!(
            findings("(cond ((eq k :a) 1) ((eq k :b) 2) ((eq k :c) 3))").len(),
            1
        );
        assert_eq!(
            findings("(cond ((eql c #\\a) 1) ((eql c #\\b) 2) ((eql c #\\c) 3))").len(),
            1
        );
        assert_eq!(
            findings("(cond ((eq s 'a) 1) ((eq s 'b) 2) ((eq s 'c) 3))").len(),
            1
        );
    }

    #[test]
    fn equal_against_eql_dependable_keys_is_convertible() {
        assert_eq!(
            findings("(cond ((equal n 1) :a) ((equal n 2) :b) ((equal n 3) :c))").len(),
            1
        );
    }

    #[test]
    fn a_package_qualified_and_case_folded_spelling_is_one_variable() {
        assert_eq!(
            findings("(COND ((cl:eql op 1) :a) ((eql OP 2) :b) ((eql app::op 3) :c))").len(),
            1
        );
    }

    #[test]
    fn finds_a_cond_nested_in_a_function_body() {
        assert_eq!(
            findings("(defun f (op) (cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c)))").len(),
            1
        );
    }

    // -- near-miss negatives -------------------------------------------------

    /// The trap this rule exists to avoid: `equal` on strings is not `eql`, so
    /// `case` would not reproduce it.
    #[test]
    fn does_not_flag_string_keys() {
        assert!(
            findings(r#"(cond ((equal s "a") 1) ((equal s "b") 2) ((equal s "c") 3))"#).is_empty()
        );
    }

    #[test]
    fn does_not_flag_float_keys() {
        assert!(findings("(cond ((eql x 1.0) 1) ((eql x 2.0) 2) ((eql x 3.0) 3))").is_empty());
    }

    #[test]
    fn does_not_flag_when_the_variable_differs_between_clauses() {
        assert!(findings("(cond ((eql a 1) :x) ((eql b 2) :y) ((eql a 3) :z))").is_empty());
    }

    /// `(pop s)` has a side effect, and `case` would evaluate it once rather
    /// than once per clause.
    #[test]
    fn does_not_flag_a_compound_test_operand() {
        assert!(
            findings("(cond ((eql (pop s) 1) :a) ((eql (pop s) 2) :b) ((eql (pop s) 3) :c))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_cond_with_a_non_comparison_clause() {
        assert!(
            findings("(cond ((eql op 1) :a) ((plusp op) :b) ((eql op 3) :c) (t :d))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_comparator_case_cannot_reproduce() {
        assert!(findings("(cond ((= n 1) :a) ((= n 2) :b) ((= n 3) :c))").is_empty());
        assert!(
            findings("(cond ((string= s \"a\") 1) ((string= s \"b\") 2) ((string= s \"c\") 3))")
                .is_empty()
        );
    }

    /// `(cond ((eql x 1)))` returns `t`; `(case x (1))` returns `nil`.
    #[test]
    fn does_not_flag_a_body_less_clause() {
        assert!(findings("(cond ((eql op 1)) ((eql op 2) :b) ((eql op 3) :c))").is_empty());
    }

    #[test]
    fn does_not_flag_a_t_clause_that_is_not_final() {
        assert!(findings("(cond ((eql op 1) :a) (t :mid) ((eql op 3) :c))").is_empty());
    }

    /// `otherwise` is an ordinary variable in `cond`, not a catch-all.
    #[test]
    fn does_not_treat_otherwise_as_a_catch_all() {
        assert!(
            findings("(cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c) (otherwise :d))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_fewer_than_three_comparison_clauses() {
        assert!(findings("(cond ((eql op 1) :a) ((eql op 2) :b))").is_empty());
        assert!(findings("(cond ((eql op 1) :a) ((eql op 2) :b) (t :c))").is_empty());
    }

    #[test]
    fn does_not_flag_t_or_nil_as_a_key() {
        assert!(findings("(cond ((eq x t) 1) ((eq x 'a) 2) ((eq x 'b) 3))").is_empty());
        assert!(findings("(cond ((eq x nil) 1) ((eq x 'a) 2) ((eq x 'b) 3))").is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_clause() {
        assert!(
            findings("(cond (#+sbcl (eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_two_variables_compared_to_each_other() {
        assert!(findings("(cond ((eql a b) 1) ((eql a c) 2) ((eql a d) 3))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_clause() {
        assert!(findings("(cond ((eql op 1) :a) op ((eql op 3) :c))").is_empty());
    }

    // -- the five quote shapes -----------------------------------------------

    const CANDIDATE: &str = "(cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c))";

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

    // -- a string literal is one atom ----------------------------------------

    #[test]
    fn a_cond_inside_a_string_literal_is_not_a_form() {
        let built = report(&format!("(format t \"{}\")", CANDIDATE.replace('"', "")));
        assert!(built.findings.is_empty());
    }

    // -- envelope ------------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CANDIDATE, Dialect::Clojure).expect("parse");
        let built =
            build_cond_to_case_candidate_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let built = report(&format!("{CANDIDATE}\n(cond ((plusp n) 1) (t 2))\n"));
        assert_eq!(built.summary, vec![("cond_form_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_fields() {
        let built = report(&format!("(defun f (op)\n  {CANDIDATE})\n"));
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(finding.kind(), "cond-to-case-candidate");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("variable", json!("op")),
                ("comparison_clauses", json!(3_usize)),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op".to_owned(), "3".to_owned()]
        );
    }
}
