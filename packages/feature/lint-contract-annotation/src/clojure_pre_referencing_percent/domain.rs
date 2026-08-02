//! Clojure `%`-in-`:pre` detection: a precondition that refers to the return
//! value, which does not exist yet.
//!
//! `clojure.core`'s `fn` macro binds `%` in exactly one place — the `:post`
//! wrapper:
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
//! `:post` conditions are spliced *inside* `(let [% …] …)`; `:pre` conditions
//! are concatenated in front of the body with no such binding. A `%` written in
//! a `:pre` vector therefore names nothing — the function has not returned yet,
//! so there is no return value for it to mean. The author almost always meant
//! either a parameter (in `:pre`) or wanted the condition in `:post`.
//!
//! # This is usually also a compiler error, and that is worth stating plainly
//!
//! Because `:pre` conditions are emitted as ordinary code, a free `%` normally
//! makes the Clojure compiler raise `Unable to resolve symbol: % in this
//! context`. A rule that only restated a compiler error would earn little.
//! Two things keep this one worth having:
//!
//! - `assert` expands to nothing at all when `*assert*` is false at compile
//!   time, so the offending expression is never emitted and never resolved. A
//!   build with assertions disabled compiles this silently and breaks the day
//!   anyone turns them back on.
//! - This tool is a *static* linter with no Clojure toolchain behind it. An
//!   agent editing a `.clj` file it cannot compile gets the diagnosis here or
//!   not at all — and the message says what the author meant, which
//!   `Unable to resolve symbol: %` does not.
//!
//! # What is deliberately not flagged
//!
//! - **`%` inside a `#(…)` anonymous-function literal**, where `%`, `%1` … `%9`
//!   and `%&` are that literal's own parameters. `(defn f [x] {:pre [(every?
//!   #(pos? %) x)]} x)` is correct Clojure and must stay silent. A set literal
//!   `#{…}` is *not* exempted: it is `HashLiteral`-prefixed too, but it binds
//!   nothing, so a `%` inside one is the same unbound symbol.
//! - **A `defn` whose own parameter vector binds `%`.** `(defn f [%] {:pre
//!   [(pos? %)]} %)` is legal — `%` is an ordinary symbol in Clojure — and the
//!   `:pre` reference resolves to the parameter.
//! - **`:post`**, which is where `%` belongs.
//! - **A `%` bound by an enclosing `let` or `def`** outside the `defn`. Legal,
//!   pathological, and not modelled: a known false positive, and the only one
//!   this rule has.
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
    CLOJURE_DEFN_HEADS, clojure_defn_arities, clojure_map_value, is_function_literal,
    is_percent_parameter, is_unevaluated_at,
};

/// The one place this rule's dialect is decided; see
/// [`crate::clojure_pre_post_vacuous::domain::SCOPE`].
pub const SCOPE: RuleDialectScope = RuleDialectScope::CLOJURE_ONLY;

#[derive(Debug, Clone)]
pub struct ClojurePreReferencingPercentItem {
    /// The span of the offending `%` itself, which is the smallest thing that
    /// has to change.
    pub span: ByteSpan,
    /// How it was spelled: `%`, `%1`, `%&` …
    pub name: String,
}

impl Finding for ClojurePreReferencingPercentItem {
    fn kind(&self) -> &'static str {
        "clojure-pre-referencing-percent"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("name={}", self.name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("name", json!(self.name))]
    }

    fn message(&self) -> String {
        format!(
            "`{}` in a :pre condition names nothing: clojure.core's fn binds % only inside the \
             :post wrapper, so there is no return value yet — move the condition to :post, or \
             name a parameter",
            self.name
        )
    }
}

/// Collects the `%`-family symbols referenced directly by a `:pre` vector,
/// skipping any that a `#(…)` literal binds.
///
/// Iterative rather than recursive: a deeply nested condition must not depend
/// on stack depth.
fn percent_references(vector: &ExpressionView, found: &mut Vec<(ByteSpan, String)>) {
    let mut stack = vec![vector];
    while let Some(view) = stack.pop() {
        // A `#(…)` literal binds `%`, `%1` … `%&` itself, so nothing inside it
        // is the return value. Do not descend.
        if is_function_literal(view) {
            continue;
        }
        if let Some(text) = atom_text(view) {
            if is_percent_parameter(text) {
                found.push((view.span, text.to_owned()));
            }
        }
        for child in view.children.iter().rev() {
            stack.push(child);
        }
    }
}

/// Whether this arity's parameter vector itself binds a `%`-family name, in
/// which case the `:pre` reference resolves to that parameter and is fine.
///
/// Only the vector's own top-level entries are read. A destructuring form that
/// bound `%` somewhere inside would be missed, which makes the rule quieter,
/// never louder.
fn params_bind_percent(params: &ExpressionView) -> bool {
    params
        .children
        .iter()
        .filter_map(atom_text)
        .any(is_percent_parameter)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Counts every `:pre` vector scanned as the denominator.
pub fn examine_defn(
    view: &ExpressionView,
    pre_condition_count: &mut usize,
    violations: &mut Vec<ClojurePreReferencingPercentItem>,
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
        let Some(vector) = clojure_map_value(conditions, ":pre") else {
            continue;
        };
        *pre_condition_count += 1;
        if params_bind_percent(arity.params) {
            continue;
        }
        let mut found = Vec::new();
        percent_references(vector, &mut found);
        violations.extend(
            found
                .into_iter()
                .map(|(span, name)| ClojurePreReferencingPercentItem { span, name }),
        );
    }
}

/// Collects every `%` referenced from a `:pre` vector in one file, with the
/// number of `:pre` vectors scanned as the denominator beside them.
///
/// `dialect_modelled` is derived from [`SCOPE`], the same constant the engine
/// consults, so scope and report cannot drift.
pub fn build_clojure_pre_referencing_percent_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ClojurePreReferencingPercentItem>> {
    let modelled = SCOPE.includes(dialect);
    let mut pre_condition_count = 0;
    let mut violations = Vec::new();

    if modelled {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            crate::support::for_each_evaluated_subview(&view, |subview| {
                examine_defn(subview, &mut pre_condition_count, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("pre_condition_count", json!(pre_condition_count))],
    ))
}

/// The rule's own data guard, shared with the report so the two agree.
#[must_use]
pub fn is_data_at(tree: &SyntaxTree, span: ByteSpan) -> bool {
    is_unevaluated_at(tree, span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ClojurePreReferencingPercentItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_clojure_pre_referencing_percent_report(Path::new("core.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<ClojurePreReferencingPercentItem> {
        report(input).findings
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_bare_percent_in_a_pre_vector() {
        let found = findings("(defn f [x] {:pre [(pos? %)]} x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "%");
    }

    #[test]
    fn flags_the_numbered_and_rest_spellings() {
        assert_eq!(findings("(defn f [x] {:pre [(pos? %1)]} x)")[0].name, "%1");
        assert_eq!(findings("(defn f [x] {:pre [(seq %&)]} x)")[0].name, "%&");
    }

    #[test]
    fn flags_a_percent_nested_deep_inside_a_condition() {
        assert_eq!(
            findings("(defn f [x] {:pre [(and (map? x) (pos? (count %)))]} x)").len(),
            1
        );
    }

    #[test]
    fn flags_each_occurrence() {
        assert_eq!(
            findings("(defn f [x] {:pre [(pos? %) (int? %)]} x)").len(),
            2
        );
    }

    #[test]
    fn flags_the_offending_arity_of_a_multi_arity_defn() {
        let found = findings("(defn f ([x] {:pre [(pos? x)]} x) ([x y] {:pre [(pos? %)]} y))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn the_span_covers_only_the_percent_symbol() {
        let source = "(defn f [x] {:pre [(pos? %)]} x)";
        let found = findings(source);
        let slice = &source[found[0].span.start().get()..found[0].span.end().get()];
        assert_eq!(slice, "%");
    }

    // -- near-miss negatives -------------------------------------------------

    /// The one that matters most: `%` inside `#(…)` is that literal's own
    /// parameter, and this is idiomatic, correct Clojure.
    #[test]
    fn does_not_flag_a_percent_inside_a_function_literal() {
        assert!(findings("(defn f [xs] {:pre [(every? #(pos? %) xs)]} xs)").is_empty());
        assert!(findings("(defn f [xs] {:pre [(some #(= % 1) xs)]} xs)").is_empty());
    }

    /// A `%` beside a legitimate one inside a literal is still reported: the
    /// exemption is scoped to the literal, not to the whole condition.
    #[test]
    fn flags_a_bare_percent_beside_an_exempt_one() {
        let found = findings("(defn f [xs] {:pre [(and (every? #(pos? %) xs) (pos? %))]} xs)");
        assert_eq!(found.len(), 1);
    }

    /// `%` is an ordinary symbol in Clojure, so a parameter may be named `%`.
    #[test]
    fn does_not_flag_when_the_parameter_vector_binds_percent() {
        assert!(findings("(defn f [%] {:pre [(pos? %)]} %)").is_empty());
    }

    /// `:post` is exactly where `%` belongs.
    #[test]
    fn does_not_flag_a_percent_in_post() {
        assert!(findings("(defn f [x] {:post [(pos? %)]} x)").is_empty());
        assert!(findings("(defn f [x] {:pre [(pos? x)] :post [(pos? %)]} x)").is_empty());
    }

    /// A lone trailing map is the return value, not a condition map, so its
    /// contents are never conditions at all.
    #[test]
    fn does_not_flag_a_lone_trailing_map() {
        assert!(findings("(defn f [x] {:pre [(pos? %)]})").is_empty());
    }

    #[test]
    fn does_not_flag_a_percent_free_precondition() {
        assert!(findings("(defn f [x] {:pre [(pos? x) (int? x)]} x)").is_empty());
    }

    /// A set literal is `#`-prefixed too, but binds nothing — a `%` inside one
    /// is the same unbound symbol, and is reported.
    #[test]
    fn a_set_literal_is_not_a_function_literal() {
        assert_eq!(
            findings("(defn f [x] {:pre [(contains? #{%} x)]} x)").len(),
            1
        );
    }

    #[test]
    fn does_not_flag_a_symbol_that_merely_contains_a_percent() {
        assert!(findings("(defn f [x] {:pre [(pos? pct%)]} x)").is_empty());
        assert!(findings("(defn f [x] {:pre [(pos? %pct)]} x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_defn_head() {
        assert!(findings("(defmacro f [x] {:pre [(pos? %)]} x)").is_empty());
    }

    // -- the five quote shapes, in Clojure's spelling ------------------------

    #[test]
    fn a_quoted_defn_is_data_and_is_not_flagged() {
        assert!(findings("'(defn f [x] {:pre [(pos? %)]} x)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings("(quote (defn f [x] {:pre [(pos? %)]} x))").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data_in_clojure() {
        assert!(findings("'(a ,(defn f [x] {:pre [(pos? %)]} x))").is_empty());
    }

    #[test]
    fn a_syntax_quote_without_an_unquote_is_data() {
        assert!(findings("`(defn f [x] {:pre [(pos? %)]} x)").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_syntax_quote_is_code_again() {
        assert_eq!(findings("`(a ~(defn f [x] {:pre [(pos? %)]} x))").len(), 1);
    }

    // -- a string literal ----------------------------------------------------

    #[test]
    fn a_defn_spelled_inside_a_string_is_text_not_a_form() {
        assert!(findings("(println \"(defn f [x] {:pre [(pos? %)]} x)\")").is_empty());
    }

    // -- the wrong dialect ---------------------------------------------------

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        assert!(
            SyntaxTree::parse_with_dialect("(defn f [x] {:pre [(pos? %)]} x)", Dialect::CommonLisp)
                .is_err()
        );

        let tree =
            SyntaxTree::parse_with_dialect("(defn f (x) nil)", Dialect::CommonLisp).expect("parse");
        let report = build_clojure_pre_referencing_percent_report(
            Path::new("app.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("pre_condition_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(defn f [x] x)").dialect_modelled);
    }

    // -- the envelope --------------------------------------------------------

    #[test]
    fn the_summary_counts_every_pre_vector_not_only_the_offending_ones() {
        let report = report(
            "(defn a [x] {:pre [(pos? x)]} x)\n\
             (defn b [x] {:pre [(pos? %)]} x)\n\
             (defn c [x] {:post [(pos? %)]} x)\n",
        );
        assert_eq!(report.summary, vec![("pre_condition_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(ns app.core)\n(defn f [x]\n  {:pre [(pos? %)]}\n  x)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "clojure-pre-referencing-percent");
        assert_eq!(finding.json_fields(), vec![("name", json!("%"))]);
        assert_eq!(finding.text_columns(), vec!["name=%".to_owned()]);
        assert!(finding.message().contains(":post"));
    }
}
