//! An `apply` whose argument sequence is written out as a literal.
//!
//! `(apply f [1 2 3])` builds a vector, then takes it apart again to call `f`
//! with three arguments. `(f 1 2 3)` says the same thing directly, and says it
//! at the call site rather than one indirection away.
//!
//! ```clojure
//! (apply str ["a" "b"])       ; (str "a" "b")
//! (apply max 1 [2 3])         ; (max 1 2 3)
//! ```
//!
//! # What it looks at
//!
//! `apply`'s last argument, and only when it is an **ordered literal**: a `[…]`
//! vector or a `'(…)` quoted list.
//!
//! # The soundness boundary, which is the whole rule
//!
//! `(apply f #{1 2 3})` is **not** reported, and this is not conservatism — it
//! is that the rewrite does not exist. A Clojure set has no specified iteration
//! order, so `#{1 2 3}` spreads into *some* order the reader may not predict;
//! writing `(f 1 2 3)` would pick one and call it the program's meaning. A map
//! literal is worse: `apply` spreads `MapEntry` values, so `(apply f {:a 1})`
//! is `(f [:a 1])` and not `(f :a 1)` at all. [`is_ordered_literal_sequence`]
//! admits exactly the two spellings whose order is defined.
//!
//! [`is_ordered_literal_sequence`]: crate::support::is_ordered_literal_sequence
//!
//! # Things that look like the defect and are not
//!
//! - **A computed sequence.** `(apply f xs)`, `(apply f (vals m))`,
//!   `(apply concat colls)` — the whole point of `apply` and never reported.
//! - **A literal that is not a sequence.** See above.
//! - **A macro template.** `` `(apply f [~@args]) `` is data, and the quote
//!   model declines it.
//!
//! # What it does not attempt
//!
//! - **`(list …)` and `(vector …)` calls.** `(apply f (list 1 2))` is the same
//!   redundancy dressed as a call rather than a literal. It is not reported,
//!   because "the argument list is written out at the call site" is a claim
//!   about *literal* syntax, and widening it to constructor calls means
//!   deciding whether `(vector-of :int 1 2)` counts. The narrower claim was
//!   chosen.
//! - **Arity.** `(apply f [])` is reported as `(f)`; whether `f` accepts zero
//!   arguments is `arity`'s question, not this one's.
//!
//! # A premise that was checked and held
//!
//! `apply` is a **function**, so `f` is always a value: a macro cannot be
//! applied at all (`(apply and [true false])` does not compile), which means no
//! valid program can reach this rule with an `f` whose rewrite would change
//! evaluation. Variadic and multi-arity functions receive the same arguments
//! either way — `apply` spreads the sequence into positional arguments, and a
//! `[& args]` tail re-collects them exactly as a direct call would.
//!
//! Scope: Clojure only. Common Lisp's `apply` has the same shape, but its
//! literal spellings are different (`'(1 2 3)` and `#(1 2 3)` rather than
//! `[…]`), `#(…)` there is a *vector* rather than a reader lambda, and the
//! Common Lisp rules live in their own packages.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, head_is, is_ordered_literal_sequence, skip_metadata_forms,
    symbol_name,
};

/// The one head this rule anchors on.
pub const APPLY_HEADS: &[&str] = &["apply"];

#[derive(Debug, Clone)]
pub struct ApplyWithLiteralCollectionItem {
    /// The span of the whole `(apply …)` form.
    pub span: ByteSpan,
    /// The function being applied, when it is named literally.
    pub applied: String,
    /// How many arguments the literal spreads.
    pub spread_count: usize,
}

impl Finding for ApplyWithLiteralCollectionItem {
    fn kind(&self) -> &'static str {
        "apply-with-literal-collection"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("applied={}", self.applied),
            format!("spread_count={}", self.spread_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("applied", json!(self.applied)),
            ("spread_count", json!(self.spread_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "apply spreads a literal of {} element(s), so {} can be called directly",
            self.spread_count, self.applied
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_apply(
    view: &ExpressionView,
    apply_count: &mut usize,
    violations: &mut Vec<ApplyWithLiteralCollectionItem>,
) {
    if !head_is(view, APPLY_HEADS) {
        return;
    }
    *apply_count += 1;

    // `(apply f … coll)`: at minimum a function and a sequence.
    if view.children.len() < 3 {
        return;
    }
    let Some(sequence) = view.children.last() else {
        return;
    };
    if !is_ordered_literal_sequence(sequence) {
        return;
    }
    // Past any metadata: the reader makes `^:x` a sibling of what it annotates,
    // so `(apply ^:x f […])` has the function at child 2.
    let applied = skip_metadata_forms(&view.children)
        .nth(1)
        .and_then(symbol_name)
        .unwrap_or_else(|| "the applied function".to_owned());

    violations.push(ApplyWithLiteralCollectionItem {
        span: view.span,
        applied,
        spread_count: sequence.children.len(),
    });
}

/// Collects every `apply` over a literal sequence in one file, with the number
/// of `apply` forms scanned as the denominator beside them.
pub fn build_apply_with_literal_collection_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ApplyWithLiteralCollectionItem>> {
    let mut apply_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_apply(view, &mut apply_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("apply_count", json!(apply_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ApplyWithLiteralCollectionItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_apply_with_literal_collection_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<ApplyWithLiteralCollectionItem> {
        report(input).findings
    }

    fn applied(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.applied)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_a_vector_literal_argument_sequence() {
        assert_eq!(applied("(apply f [1 2 3])"), vec!["f".to_owned()]);
        assert_eq!(applied("(apply str [\"a\" \"b\"])"), vec!["str".to_owned()]);
    }

    #[test]
    fn flags_a_quoted_list_argument_sequence() {
        assert_eq!(applied("(apply + '(1 2 3))"), vec!["+".to_owned()]);
    }

    #[test]
    fn flags_a_literal_after_leading_positional_arguments() {
        let items = violations("(apply max 1 [2 3])");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].spread_count, 2);
    }

    #[test]
    fn flags_an_empty_literal() {
        let items = violations("(apply f [])");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].spread_count, 0);
    }

    #[test]
    fn flags_a_keyword_or_reader_lambda_in_function_position() {
        assert_eq!(applied("(apply :k [m])"), vec![":k".to_owned()]);
        assert_eq!(
            applied("(apply #(+ %1 %2) [1 2])"),
            vec!["the applied function".to_owned()]
        );
    }

    // --- the soundness boundary ----------------------------------------------

    /// A set has no specified iteration order, so `(f 1 2 3)` is not a
    /// rewrite of `(apply f #{1 2 3})` — it is a *choice* of one of several
    /// possible meanings. Never reported.
    #[test]
    fn does_not_flag_a_set_literal_whose_order_is_undefined() {
        assert!(violations("(apply f #{1 2 3})").is_empty());
        assert!(violations("(apply str #{\"a\" \"b\"})").is_empty());
    }

    /// `apply` spreads a map's `MapEntry` values, so `(apply f {:a 1})` is
    /// `(f [:a 1])` and the "say it directly" rewrite would be wrong.
    #[test]
    fn does_not_flag_a_map_literal_whose_elements_are_entries() {
        assert!(violations("(apply f {:a 1 :b 2})").is_empty());
    }

    /// `(1 2 3)` unquoted is a *call* to `1`, not a literal sequence.
    #[test]
    fn does_not_flag_an_unquoted_list_which_is_a_call() {
        assert!(violations("(apply f (build 1 2 3))").is_empty());
        assert!(violations("(apply f (list 1 2 3))").is_empty());
    }

    // --- realistic, correct Clojure that must stay silent --------------------

    #[test]
    fn does_not_flag_a_computed_argument_sequence() {
        assert!(violations("(apply f xs)").is_empty());
        assert!(violations("(apply max (map :score players))").is_empty());
        assert!(violations("(apply concat colls)").is_empty());
        assert!(violations("(apply str (interpose \", \" names))").is_empty());
        assert!(violations("(apply hash-map (vals m))").is_empty());
    }

    #[test]
    fn does_not_flag_an_apply_with_nothing_to_spread() {
        assert!(violations("(apply)").is_empty());
        assert!(violations("(apply f)").is_empty());
    }

    /// `(apply [1 2])` is malformed — `apply` needs a function *and* a
    /// sequence — and "so [1 2] can be called directly" would be nonsense.
    ///
    /// Found by mutation testing: the arity floor could be deleted with the
    /// suite green, because every existing short-call test used a spelling
    /// whose last child was not a literal.
    #[test]
    fn does_not_flag_a_two_element_apply_whose_only_argument_is_a_literal() {
        assert!(violations("(apply [1 2])").is_empty());
        assert!(violations("(apply '(1 2))").is_empty());
    }

    /// The reader makes `^:x` a **sibling** of what it annotates, so
    /// `(apply ^:x f […])` has the function at child 2. Reading child 1
    /// unconditionally names the metadata as the applied function.
    ///
    /// This is not hypothetical: the corpus audit found
    /// `(apply ^[] clojure.test.SwissArmy/doppelganger [])` in Clojure's own
    /// test suite, where `^[]` is a 1.12 param-tag. Found by mutation testing.
    #[test]
    fn names_the_applied_function_past_any_metadata() {
        let items = violations("(apply ^:x f [1 2])");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].applied, "f");
    }

    /// `#(…)` is a paren list carrying a hash prefix, not a quote — so it is
    /// not an ordered literal, and a rule that tested "paren list with any
    /// reader prefix" would report it.
    #[test]
    fn does_not_flag_a_reader_lambda_in_the_sequence_position() {
        assert!(violations("(apply f #(+ % 1))").is_empty());
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(apply f [1 2 3])").is_empty());
        assert!(violations("(quote (apply f [1 2 3]))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(apply f [1 2 3])").is_empty());
    }

    /// The shape a macro writing an `apply` actually has, and the one a naive
    /// depth counter gets wrong.
    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(violations("'(a ,(apply f [1 2 3]))").is_empty());
        assert!(violations("`(a ,(apply f [1 2 3]))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(applied("`(do ~(apply f [1 2 3]))"), vec!["f".to_owned()]);
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(println \"(apply f [1 2 3])\")").is_empty());
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_apply_scanned() {
        let report = report("(apply f xs)\n(apply g [1 2])\n");
        assert_eq!(report.summary, vec![("apply_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_applied_and_spread_count() {
        let report = report("(defn go []\n  (apply str [\"a\" \"b\"]))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "apply-with-literal-collection");
        assert_eq!(
            finding.json_fields(),
            vec![("applied", json!("str")), ("spread_count", json!(2))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["applied=str".to_owned(), "spread_count=2".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "apply spreads a literal of 2 element(s), so str can be called directly"
        );
    }

    /// A Clojure-only rule must say "nothing was looked for" rather than
    /// "nothing was found" when handed another dialect.
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(apply #'+ '(1 2 3))", Dialect::CommonLisp)
            .expect("parse");
        let report = build_apply_with_literal_collection_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("apply_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(apply f xs)").dialect_modelled);
    }
}
