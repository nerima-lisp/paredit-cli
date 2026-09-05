//! Clojure vacuous-`:pre`/`:post` detection: a condition vector that asserts
//! nothing, so the contract it looks like is not one.
//!
//! `clojure.core`'s `fn` macro turns each element of a `:pre` or `:post` vector
//! into an `(assert …)`:
//!
//! ```text
//! body (if post
//!        `((let [~'% ~(if (< 1 (count body)) `(do ~@body) (first body))]
//!            ~@(map (fn* [c] `(assert ~c)) post)
//!            ~'%))
//!        body)
//! body (if pre
//!        (concat (map (fn* [c] `(assert ~c)) pre) body)
//!        body)
//! ```
//!
//! Two vectors therefore assert nothing at all:
//!
//! - `[]` — `(map … [])` is empty, so nothing is concatenated. Note that `[]`
//!   is *truthy* in Clojure, so the `(if pre …)` branch is still taken; the
//!   emptiness is what makes it a no-op, not a skipped branch.
//! - `[true]` — `(assert true)` can never fail.
//!
//! A vector every one of whose elements is the literal `true` is the same
//! no-op, and is flagged for the same reason.
//!
//! # What is deliberately not flagged
//!
//! - **A lone trailing map.** In `(defn f [x] {:pre [true]})` the map is the
//!   function's *return value*, not a condition map — `fn` reads it as one only
//!   when `(next body)` is non-nil. Nothing is asserted there because nothing
//!   was ever meant to be.
//! - **Anything that is not the literal `true`.** `[(constantly true)]`,
//!   `[:keyword]` and `[1]` are all truthy and so all vacuous in practice, but
//!   only `true` is vacuous *by inspection*; the others could be a name this
//!   file rebinds. False negatives, on purpose.
//! - **A `:pre`/`:post` written as metadata on the parameter vector**, which
//!   `fn` also accepts (`conds (or conds (meta params))`). Not read at all.
//! - **`nil` or a non-vector value.** `{:pre nil}` and `{:pre (foo)}` are not
//!   the shape this rule models, and guessing at them would be guessing.
//!
//! Scope: Clojure only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::policy::RuleDialectScope;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::atom_text;
use serde_json::{Value, json};

use crate::support::{
    CLOJURE_DEFN_HEADS, clojure_defn_arities, clojure_map_value, is_bracket_list, is_unevaluated_at,
};

/// The one place this rule's dialect is decided.
///
/// Both [`crate::clojure_pre_post_vacuous::rule::Rule::dialect_scope`] and
/// [`build_clojure_pre_post_vacuous_report`]'s `dialect_modelled` flag read it,
/// so the engine's view of which dialects this rule runs for and the standalone
/// report's claim about which dialects it measured cannot drift apart.
pub const SCOPE: RuleDialectScope = RuleDialectScope::CLOJURE_ONLY;

/// The two condition keys `fn` recognises.
const CONDITION_KEYS: [&str; 2] = [":pre", ":post"];

/// Why one condition vector asserts nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacuousShape {
    /// `[]` — no conditions at all.
    Empty,
    /// `[true]`, or any vector all of whose elements are the literal `true`.
    AlwaysTrue,
}

impl VacuousShape {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::AlwaysTrue => "always-true",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClojurePrePostVacuousItem {
    /// The span of the condition vector itself, which is what a reader has to
    /// look at to fix it.
    pub span: ByteSpan,
    /// `:pre` or `:post`.
    pub key: String,
    pub shape: VacuousShape,
}

impl Finding for ClojurePrePostVacuousItem {
    fn kind(&self) -> &'static str {
        "clojure-pre-post-vacuous"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("key={}", self.key),
            format!("shape={}", self.shape.as_str()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("key", json!(self.key)),
            ("shape", json!(self.shape.as_str())),
        ]
    }

    fn message(&self) -> String {
        let detail = match self.shape {
            VacuousShape::Empty => "the vector is empty, so nothing is asserted",
            VacuousShape::AlwaysTrue => "every condition is the literal true, which cannot fail",
        };
        format!(
            "vacuous {} contract: {detail}; each element becomes an (assert …), so this one is a no-op",
            self.key
        )
    }
}

/// Whether `view` is the literal `true`.
fn is_true_literal(view: &ExpressionView) -> bool {
    atom_text(view) == Some("true")
}

/// Classifies one condition vector, or `None` if it does assert something.
fn vacuous_shape(vector: &ExpressionView) -> Option<VacuousShape> {
    if !is_bracket_list(vector) {
        return None;
    }
    if vector.children.is_empty() {
        return Some(VacuousShape::Empty);
    }
    vector
        .children
        .iter()
        .all(is_true_literal)
        .then_some(VacuousShape::AlwaysTrue)
}

///
/// Counts every `defn` arity that carries a condition map as the denominator:
/// "two vacuous contracts" means something different in a file with three
/// contracts than in one with thirty.
pub fn examine_defn(
    view: &ExpressionView,
    contract_count: &mut usize,
    violations: &mut Vec<ClojurePrePostVacuousItem>,
) {
    let is_defn = view
        .children
        .first()
        .and_then(atom_text)
        .is_some_and(|head| CLOJURE_DEFN_HEADS.contains(&head));
    if !is_defn {
        return;
    }

    for arity in clojure_defn_arities(view) {
        let Some(conditions) = arity.conditions else {
            continue;
        };
        *contract_count += 1;
        for key in CONDITION_KEYS {
            let Some(vector) = clojure_map_value(conditions, key) else {
                continue;
            };
            if let Some(shape) = vacuous_shape(vector) {
                violations.push(ClojurePrePostVacuousItem {
                    span: vector.span,
                    key: key.to_owned(),
                    shape,
                });
            }
        }
    }
}

/// Collects every vacuous `:pre`/`:post` vector in one file, with the number of
/// condition maps scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_clojure_pre_post_vacuous_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ClojurePrePostVacuousItem>> {
    let modelled = SCOPE.includes(dialect);
    let mut contract_count = 0;
    let mut violations = Vec::new();

    if modelled {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            crate::support::for_each_evaluated_subview(&view, |subview| {
                examine_defn(subview, &mut contract_count, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("contract_count", json!(contract_count))],
    ))
}

/// The rule's own guard, shared with the report so the two agree about which
/// nodes are code.
#[must_use]
pub fn is_data_at(tree: &SyntaxTree, span: ByteSpan) -> bool {
    is_unevaluated_at(tree, span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ClojurePrePostVacuousItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_clojure_pre_post_vacuous_report(Path::new("core.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<ClojurePrePostVacuousItem> {
        report(input).findings
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_an_empty_pre_vector() {
        let found = findings("(defn f [x] {:pre []} x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, ":pre");
        assert_eq!(found[0].shape, VacuousShape::Empty);
    }

    #[test]
    fn flags_a_pre_vector_of_literal_true() {
        let found = findings("(defn f [x] {:pre [true]} x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].shape, VacuousShape::AlwaysTrue);
    }

    #[test]
    fn flags_a_vacuous_post_vector_too() {
        let found = findings("(defn f [x] {:post [true]} x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, ":post");
    }

    #[test]
    fn flags_both_keys_of_one_map_independently() {
        let found = findings("(defn f [x] {:pre [] :post [true]} x)");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].key, ":pre");
        assert_eq!(found[1].key, ":post");
    }

    #[test]
    fn flags_a_vector_whose_every_element_is_true() {
        let found = findings("(defn f [x] {:pre [true true]} x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].shape, VacuousShape::AlwaysTrue);
    }

    #[test]
    fn flags_the_offending_arity_of_a_multi_arity_defn() {
        let found = findings("(defn f ([x] {:pre [(pos? x)]} x) ([x y] {:pre [true]} (+ x y)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].shape, VacuousShape::AlwaysTrue);
    }

    #[test]
    fn flags_a_private_defn_too() {
        assert_eq!(findings("(defn- f [x] {:pre [true]} x)").len(), 1);
    }

    #[test]
    fn the_span_covers_the_condition_vector_not_the_whole_defn() {
        let source = "(defn f [x] {:pre [true]} x)";
        let found = findings(source);
        let slice = &source[found[0].span.start().get()..found[0].span.end().get()];
        assert_eq!(slice, "[true]");
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_real_condition() {
        assert!(findings("(defn f [x] {:pre [(pos? x)]} x)").is_empty());
        assert!(findings("(defn f [x] {:post [(pos? %)]} x)").is_empty());
    }

    /// The near-miss that matters most: `true` among real conditions is not a
    /// vacuous contract, because the others still assert something.
    #[test]
    fn does_not_flag_a_true_beside_a_real_condition() {
        assert!(findings("(defn f [x] {:pre [true (pos? x)]} x)").is_empty());
    }

    /// `fn` reads a brace map as a condition map only when something follows
    /// it. A lone map is the return value, and flagging it would report a
    /// function that legitimately returns `{:pre [true]}`.
    #[test]
    fn does_not_flag_a_lone_trailing_map_which_is_the_return_value() {
        assert!(findings("(defn f [x] {:pre []})").is_empty());
        assert!(findings("(defn f [x] {:pre [true]})").is_empty());
    }

    /// An attribute map sits before the parameter vector and is not a contract
    /// at all.
    #[test]
    fn does_not_flag_an_attribute_map() {
        assert!(findings("(defn f \"doc\" {:added \"1.0\"} [x] x)").is_empty());
        assert!(findings("(defn f {:pre []} [x] x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_truthy_non_literal() {
        // Vacuous in practice, but not by inspection: `always` could be
        // anything this file binds.
        assert!(findings("(defn f [x] {:pre [always]} x)").is_empty());
        assert!(findings("(defn f [x] {:pre [1]} x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_vector_value() {
        assert!(findings("(defn f [x] {:pre nil} x)").is_empty());
        assert!(findings("(defn f [x] {:pre (build-conditions)} x)").is_empty());
    }

    #[test]
    fn does_not_flag_an_unrelated_map_key() {
        assert!(findings("(defn f [x] {:doc []} x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_defn_without_any_condition_map() {
        assert!(findings("(defn f [x y] (+ x y))").is_empty());
    }

    /// `defmacro` and `def` take neither shape; only `defn`/`defn-` do.
    #[test]
    fn does_not_flag_a_non_defn_head() {
        assert!(findings("(defmacro f [x] {:pre [true]} x)").is_empty());
        assert!(findings("(deftest f [x] {:pre [true]} x)").is_empty());
    }

    // -- the five quote shapes, in Clojure's spelling ------------------------

    #[test]
    fn a_quoted_defn_is_data_and_is_not_flagged() {
        assert!(findings("'(defn f [x] {:pre [true]} x)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings("(quote (defn f [x] {:pre [true]} x))").is_empty());
    }

    /// In Clojure `,` is whitespace, not an unquote — so this is a hard quote
    /// with a stray comma, and all of it is data.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data_in_clojure() {
        assert!(findings("'(a ,(defn f [x] {:pre [true]} x))").is_empty());
    }

    /// Clojure's unquote is `~`. A syntax-quoted template with no unquote is
    /// all data.
    #[test]
    fn a_syntax_quote_without_an_unquote_is_data() {
        assert!(findings("`(defn f [x] {:pre [true]} x)").is_empty());
    }

    /// `~` escapes back to code, and the escaped form is checked.
    #[test]
    fn an_unquote_inside_a_syntax_quote_is_code_again() {
        assert_eq!(findings("`(a ~(defn f [x] {:pre [true]} x))").len(), 1);
    }

    // -- a string literal ----------------------------------------------------

    #[test]
    fn a_defn_spelled_inside_a_string_is_text_not_a_form() {
        assert!(findings("(println \"(defn f [x] {:pre [true]} x)\")").is_empty());
    }

    // -- the wrong dialect ---------------------------------------------------

    /// The scope is a declaration, and the standalone report reads the same
    /// constant, so a Common Lisp file is reported as unmodelled rather than
    /// as clean.
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        // `[`, `]`, `{` and `}` are constituent characters in Common Lisp, not
        // delimiters, so the Clojure spelling still parses as Common Lisp —
        // just as a run of ordinary symbols rather than a `defn` this rule
        // recognizes. The Common Lisp shape closest to it still reaches no
        // finding.
        assert!(
            SyntaxTree::parse_with_dialect("(defn f [x] {:pre [true]} x)", Dialect::CommonLisp)
                .is_ok()
        );

        let tree =
            SyntaxTree::parse_with_dialect("(defn f (x) nil)", Dialect::CommonLisp).expect("parse");
        let report = build_clojure_pre_post_vacuous_report(
            Path::new("app.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("contract_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(defn f [x] x)").dialect_modelled);
    }

    // -- the envelope --------------------------------------------------------

    #[test]
    fn the_summary_counts_every_condition_map_not_only_the_vacuous_ones() {
        let report = report(
            "(defn a [x] {:pre [(pos? x)]} x)\n(defn b [x] {:pre [true]} x)\n(defn c [x] x)\n",
        );
        assert_eq!(report.summary, vec![("contract_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(ns app.core)\n(defn f [x]\n  {:pre [true]}\n  x)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "clojure-pre-post-vacuous");
        assert_eq!(
            finding.json_fields(),
            vec![("key", json!(":pre")), ("shape", json!("always-true"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["key=:pre".to_owned(), "shape=always-true".to_owned()]
        );
    }

    #[test]
    fn the_message_distinguishes_the_two_shapes() {
        assert!(
            findings("(defn f [x] {:pre []} x)")[0]
                .message()
                .contains("the vector is empty")
        );
        assert!(
            findings("(defn f [x] {:pre [true]} x)")[0]
                .message()
                .contains("literal true")
        );
    }
}
