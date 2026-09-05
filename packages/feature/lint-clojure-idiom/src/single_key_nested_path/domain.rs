//! A nested-path accessor whose path holds exactly one key.
//!
//! `assoc-in`, `update-in` and `get-in` exist to reach *through* a structure.
//! With a one-element path they reach through nothing, and each has a direct
//! one-key sibling that says so:
//!
//! ```clojure
//! (assoc-in m [:k] v)     ; (assoc m :k v)
//! (update-in m [:k] inc)  ; (update m :k inc)
//! (get-in m [:k])         ; (get m :k)
//! ```
//!
//! # The equivalence, read off `clojure/core.clj`
//!
//! Each nested form's own definition reduces to the direct form when the path
//! has one element, so the pair is identical by construction rather than by
//! resemblance:
//!
//! ```clojure
//! (defn assoc-in [m [k & ks] v]
//!   (if ks (assoc m k (assoc-in (get m k) ks v))
//!          (assoc m k v)))                          ; ← the whole one-key case
//!
//! (defn update-in [m ks f & args]
//!   (let [[k & ks] ks]
//!     (if ks (assoc m k (apply update-in (get m k) ks f args))
//!            (assoc m k (apply f (get m k) args))))) ; ≡ (update m k f & args)
//!
//! (defn get-in ([m ks] (reduce1 get m ks)) …)        ; ≡ (get m k) for one k
//! ```
//!
//! That covers the cases a reader would want to check by hand:
//!
//! - **`m` is `nil`.** `(assoc-in nil [:k] v)` takes the `else` branch and is
//!   `(assoc nil :k v)` — the same expression, so the same `{:k v}`.
//! - **`m` is a vector and the key an index.** Both spellings end at the same
//!   `assoc`, so both throw or both succeed identically.
//! - **A record, a sorted map, a transient.** Same: whatever `assoc` does with
//!   it, both forms do.
//! - **`get-in`'s three-arity.** `(get-in m [:k] nf)` and `(get m :k nf)` agree,
//!   including on a present key whose value is `nil`.
//!
//! # What it looks at
//!
//! The path argument, and only when it is a literal `[…]` vector with exactly
//! one element. A computed path — `(assoc-in m path v)`, `(get-in m (conj p
//! :k))` — is not a claim this rule can make, and a path of two or more
//! elements is what these operators are for.
//!
//! # What it does not attempt
//!
//! - **The empty path.** `(assoc-in m [] v)` destructures `k` to `nil` and is
//!   `(assoc m nil v)`, which is legal and almost certainly a mistake — but a
//!   *different* mistake, deserving a different message, and one this rule
//!   would misdescribe. `(get-in m [])` is `m`. Neither is reported here.
//! - **`update` version skew.** `update` arrived in Clojure 1.7 (2015) and
//!   `get`/`assoc` have always existed, so the suggested repair is available in
//!   every Clojure a reader is likely to be running. Nothing checks this.
//! - **`assoc-in!` / transient variants**, which are not `clojure.core`.
//!
//! Scope: Clojure only. None of `assoc-in`, `update-in` or `get-in` is a Common
//! Lisp operator, and the `[…]` path spelling is not readable there.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, head_is, is_vector_literal, normalized_symbol};

/// The nested-path operators, each with the direct operator it reduces to when
/// its path has one key.
///
/// One table rather than two lists, so the pairing cannot drift: the rule's
/// whole claim is that the left name with a one-key path *is* the right name.
pub const NESTED_PATH_OPERATORS: &[(&str, &str)] = &[
    ("assoc-in", "assoc"),
    ("update-in", "update"),
    ("get-in", "get"),
];

/// The heads [`examine_nested_path`] matches.
///
/// Spelled out as a `const` rather than derived from [`NESTED_PATH_OPERATORS`]
/// at each call. `examine_nested_path` runs on **every node** of the report
/// walk, and its first statement is the head test; building a `Vec` there
/// would be an allocation per visited node — the exact shape the `clean/forms/*`
/// benchmarks exist to catch. The test below pins the two against each other so
/// they cannot drift.
pub const NESTED_PATH_HEADS: &[&str] = &["assoc-in", "update-in", "get-in"];

/// The direct operator a nested one reduces to.
fn direct_operator(nested: &str) -> Option<&'static str> {
    NESTED_PATH_OPERATORS
        .iter()
        .find(|(name, _)| *name == nested)
        .map(|(_, direct)| *direct)
}

#[derive(Debug, Clone)]
pub struct SingleKeyNestedPathItem {
    /// The span of the whole nested-path form.
    pub span: ByteSpan,
    /// The nested operator, normalized.
    pub operator: String,
    /// The direct operator it reduces to.
    pub direct: &'static str,
}

impl Finding for SingleKeyNestedPathItem {
    fn kind(&self) -> &'static str {
        "single-key-nested-path"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("direct={}", self.direct),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("direct", json!(self.direct)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} with a one-key path is exactly {}; the nested form reaches through nothing",
            self.operator, self.direct
        )
    }
}

/// Whether the call has enough arguments for the rewrite to be the same call.
///
/// `(assoc-in m [k] v)` needs its value; `(get-in m [k])` and `(get-in m [k]
/// not-found)` are both complete; `update-in` takes a function and any number
/// of extra arguments. Anything shorter is malformed, which is a different
/// rule's subject.
fn has_complete_arity(operator: &str, children: usize) -> bool {
    match operator {
        // (assoc-in m path v)
        "assoc-in" => children == 4,
        // (update-in m path f & args)
        "update-in" => children >= 4,
        // (get-in m path) or (get-in m path not-found)
        "get-in" => children == 3 || children == 4,
        _ => false,
    }
}

pub fn examine_nested_path(
    view: &ExpressionView,
    nested_path_count: &mut usize,
    violations: &mut Vec<SingleKeyNestedPathItem>,
) {
    if !head_is(view, NESTED_PATH_HEADS) {
        return;
    }
    *nested_path_count += 1;

    let Some(operator) = list_head(view).map(normalized_symbol) else {
        return;
    };
    let Some(direct) = direct_operator(operator.as_str()) else {
        return;
    };
    if !has_complete_arity(operator.as_str(), view.children.len()) {
        return;
    }
    let Some(path) = view.children.get(2) else {
        return;
    };
    // A literal vector, and exactly one key in it. A computed path is not a
    // claim this rule can make; an empty path is a different mistake.
    if !is_vector_literal(path) || path.children.len() != 1 {
        return;
    }

    violations.push(SingleKeyNestedPathItem {
        span: view.span,
        operator,
        direct,
    });
}

/// Collects every one-key nested-path call in one file, with the number of
/// nested-path calls scanned as the denominator beside them.
pub fn build_single_key_nested_path_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleKeyNestedPathItem>> {
    let mut nested_path_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_nested_path(view, &mut nested_path_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("nested_path_count", json!(nested_path_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SingleKeyNestedPathItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_single_key_nested_path_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<SingleKeyNestedPathItem> {
        report(input).findings
    }

    fn suggestions(input: &str) -> Vec<&'static str> {
        violations(input)
            .into_iter()
            .map(|item| item.direct)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_each_nested_operator_with_its_direct_sibling() {
        assert_eq!(suggestions("(assoc-in m [:k] v)"), vec!["assoc"]);
        assert_eq!(suggestions("(update-in m [:k] inc)"), vec!["update"]);
        assert_eq!(suggestions("(get-in m [:k])"), vec!["get"]);
    }

    #[test]
    fn flags_the_optional_arities_too() {
        assert_eq!(suggestions("(get-in m [:k] :missing)"), vec!["get"]);
        assert_eq!(suggestions("(update-in m [:k] + 1 2)"), vec!["update"]);
    }

    #[test]
    fn flags_a_non_keyword_key() {
        assert_eq!(suggestions("(assoc-in v [0] x)"), vec!["assoc"]);
        assert_eq!(suggestions("(get-in m [k])"), vec!["get"]);
        assert_eq!(suggestions("(get-in m [\"name\"])"), vec!["get"]);
    }

    // --- realistic, correct Clojure that must stay silent --------------------

    #[test]
    fn does_not_flag_a_genuinely_nested_path() {
        assert!(violations("(assoc-in m [:a :b] v)").is_empty());
        assert!(violations("(update-in m [:a :b :c] inc)").is_empty());
        assert!(violations("(get-in m [:a :b])").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_path() {
        assert!(violations("(assoc-in m path v)").is_empty());
        assert!(violations("(get-in m (conj prefix :k))").is_empty());
        assert!(violations("(update-in m ks inc)").is_empty());
    }

    /// `(assoc-in m [] v)` is `(assoc m nil v)` — legal, probably a mistake,
    /// and a *different* mistake than this rule's. Reporting it here would
    /// suggest a rewrite that is not the one it needs.
    #[test]
    fn does_not_flag_an_empty_path() {
        assert!(violations("(assoc-in m [] v)").is_empty());
        assert!(violations("(get-in m [])").is_empty());
    }

    /// A path spelled as a list or a set is not a vector literal, and the
    /// element-count claim would not transfer.
    #[test]
    fn does_not_flag_a_path_that_is_not_a_vector_literal() {
        assert!(violations("(get-in m '(:k))").is_empty());
        assert!(violations("(get-in m #{:k})").is_empty());
    }

    #[test]
    fn does_not_flag_an_incomplete_call() {
        assert!(violations("(assoc-in m [:k])").is_empty());
        assert!(violations("(get-in m)").is_empty());
        assert!(violations("(update-in m [:k])").is_empty());
        // Four is `assoc-in`'s complete arity; five is malformed, and saying
        // "use assoc" about it would be wrong.
        assert!(violations("(assoc-in m [:k] v extra)").is_empty());
    }

    #[test]
    fn does_not_flag_the_direct_operators_themselves() {
        assert!(violations("(assoc m :k v)").is_empty());
        assert!(violations("(get m :k)").is_empty());
        assert!(violations("(update m :k inc)").is_empty());
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(assoc-in m [:k] v)").is_empty());
        assert!(violations("(quote (assoc-in m [:k] v))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(assoc-in m [:k] v)").is_empty());
    }

    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(violations("'(a ,(assoc-in m [:k] v))").is_empty());
        assert!(violations("`(a ,(assoc-in m [:k] v))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(suggestions("`(do ~(assoc-in m [:k] v))"), vec!["assoc"]);
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(println \"(assoc-in m [:k] v)\")").is_empty());
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_nested_path_call_scanned() {
        let report = report("(assoc-in m [:a :b] v)\n(get-in m [:k])\n(update-in m ks inc)\n");
        assert_eq!(report.summary, vec![("nested_path_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_operator_and_direct_form() {
        let report = report("(defn touch [m]\n  (update-in m [:hits] inc))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "single-key-nested-path");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("update-in")),
                ("direct", json!("update"))
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["operator=update-in".to_owned(), "direct=update".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "update-in with a one-key path is exactly update; \
             the nested form reaches through nothing"
        );
    }

    /// The table is the rule's claim; if a name were added on one side only,
    /// the pairing would be a lie. And `NESTED_PATH_HEADS` is spelled out
    /// separately for cost reasons, so it has to be pinned against the table it
    /// is a projection of — otherwise a fourth operator would be silently
    /// unreachable.
    #[test]
    fn the_head_list_is_exactly_the_left_column_of_the_table() {
        let from_table: Vec<&str> = NESTED_PATH_OPERATORS
            .iter()
            .map(|(nested, _)| *nested)
            .collect();
        assert_eq!(from_table, NESTED_PATH_HEADS);
        for (nested, direct) in NESTED_PATH_OPERATORS {
            assert!(!nested.is_empty() && !direct.is_empty());
            assert_eq!(direct_operator(nested), Some(*direct));
        }
        assert_eq!(direct_operator("assoc"), None);
    }

    /// A Clojure-only rule must say "nothing was looked for" rather than
    /// "nothing was found" when handed another dialect.
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(get-in m '(:k))", Dialect::CommonLisp).expect("parse");
        let report =
            build_single_key_nested_path_report(Path::new("a.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("nested_path_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(get-in m [:a :b])").dialect_modelled);
    }
}
