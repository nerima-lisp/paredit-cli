//! A `with-open` whose value is an unrealized sequence over the resource it is
//! about to close.
//!
//! `with-open` closes its bindings in a `finally`, so the close happens as the
//! form returns. A lazy sequence returned out of it has not been realized yet,
//! and realizing it afterwards reads from a closed stream:
//!
//! ```clojure
//! (defn lines [path]
//!   (with-open [r (io/reader path)]
//!     (map parse-line (line-seq r))))   ; IOException: Stream closed
//! ```
//!
//! The bug is unusually nasty because it is invisible on a short file that
//! happens to fit in the reader's buffer, and on a REPL where the sequence gets
//! realized by the printer before the scope exits.
//!
//! # What it looks at
//!
//! The **tail** of a `with-open`: the last body form, followed through the
//! forms whose value is their own last child ([`TAIL_POSITION_HEADS`] — `let`,
//! `do`, `when`, `->`, `->>`, …) and through both branches of an `if`. If that
//! form is a call to a `clojure.core` operation that returns an unrealized
//! sequence ([`crate::support::LAZY_SEQUENCE_HEADS`]), and it reaches one of the names
//! `with-open` bound without crossing an eager realizer, it is reported.
//!
//! # Three things that look like the defect and are not
//!
//! - **An eager tail.** `(into [] (map parse (line-seq r)))`, `(doall …)`,
//!   `(reduce + …)`, `(mapv …)` — the sequence is realized before the close.
//!   The overwhelmingly common correct spelling is `(->> (line-seq r) (map
//!   parse) (into []))`, where the threading macro's *last* stage is what the
//!   form returns.
//! - **A lazy sequence over something else.** `(with-open [r (io/reader p)]
//!   (map inc [1 2 3]))` never touches `r`, so nothing about it depends on the
//!   reader still being open. Only a tail that reaches a bound name is
//!   reported.
//! - **A lazy wrapper around realized data.** `(map parse (doall (line-seq
//!   r)))` is an unrealized `map`, but everything it will read is already in
//!   memory. The search for the resource mention stops at an eager realizer for
//!   exactly this case.
//!
//! # What it does not attempt
//!
//! - **`cond`/`case`/`condp` tails.** They have several, and picking one would
//!   be a guess; see [`TAIL_POSITION_HEADS`].
//! - **A lazy sequence returned from a helper.** `(with-open [r …] (parse-all
//!   r))` where `parse-all` is lazy is invisible without cross-function
//!   analysis, and always will be to a one-file rule.
//! - **A lazy sequence stashed in a map or vector.** `{:lines (map f
//!   (line-seq r))}` is the same bug wearing a collection literal. Reporting it
//!   would mean treating every collection tail as a container of tails, which
//!   also flags `{:count (count …)}`-shaped correct code; the narrower claim
//!   was chosen.
//! - **Escape by any other route** — a lazy seq `swap!`ed into an atom, or
//!   thrown. The tail is the route that shows up in real code.
//!
//! Scope: Clojure only. Common Lisp has `with-open-file` and no lazy
//! sequences; `line-seq`, `->>` and the rest of the vocabulary mean nothing
//! there.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{
    TAIL_POSITION_HEADS, VALUE_CARRYING_TAIL_HEADS, for_each_evaluated_subview, head_is,
    is_lazy_sequence_call, is_vector_literal, normalized_symbol, reaches_symbol_unrealized,
    symbol_name,
};

/// The one head this rule anchors on.
///
/// `with-open` is `clojure.core`'s only scope that closes what it binds.
/// `with-local-vars` and `with-redefs` restore rather than close, and neither
/// leaves a stream behind.
pub const RESOURCE_SCOPE_HEADS: &[&str] = &["with-open"];

/// How far the tail is followed through nesting forms.
///
/// A bound depth rather than an unbounded recursion: the tail chain is three or
/// four deep in the code this is about (`with-open` → `let` → `->>`), and a
/// bound turns a pathological input into a false negative instead of a stack
/// overflow.
const MAX_TAIL_DEPTH: usize = 8;

#[derive(Debug, Clone)]
pub struct WithOpenLazySeqItem {
    /// The span of the lazy tail expression, not of the whole `with-open`.
    pub span: ByteSpan,
    /// The lazy operation the tail is a call to, normalized.
    pub operation: String,
    /// The `with-open` binding the tail reaches.
    pub resource: String,
}

impl Finding for WithOpenLazySeqItem {
    fn kind(&self) -> &'static str {
        "with-open-returns-lazy-seq"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operation={}", self.operation),
            format!("resource={}", self.resource),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operation", json!(self.operation)),
            ("resource", json!(self.resource)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "with-open closes {} as it returns, so the lazy {} over it is realized too late; \
             force it inside the scope (doall/into/reduce)",
            self.resource, self.operation
        )
    }
}

/// The names a `with-open` binding vector binds.
///
/// Every even position of `[r (io/reader p) w (io/writer q)]`. A destructuring
/// binding — `[{:keys [in]} (open …)]` — yields no name here, which costs a
/// false negative and never a false positive.
fn bound_names(bindings: &ExpressionView) -> Vec<String> {
    bindings
        .children
        .iter()
        .step_by(2)
        .filter_map(symbol_name)
        .collect()
}

/// One candidate value of a `with-open` body, and the region to ask about the
/// resource in.
///
/// The two are not the same node, and conflating them was the first thing that
/// broke: in `(->> (line-seq r) (map parse))` the *lazy call* is `(map parse)`
/// and the *resource* is named two stages earlier, so asking `(map parse)`
/// whether it touches `r` answers no about the textbook spelling of the bug.
#[derive(Debug, Clone, Copy)]
struct Tail<'a> {
    /// The form whose head decides whether the value is unrealized.
    value: &'a ExpressionView,
    /// The form to search for a mention of the bound resource.
    context: &'a ExpressionView,
}

/// The forms the value of `view` can be, following the chain of forms whose
/// value is their own last child.
///
/// Both branches of an `if` are followed, because either can be the returned
/// lazy sequence and reporting only one would depend on which the author wrote
/// first.
///
/// `context` widens or narrows as the descent goes, according to
/// [`VALUE_CARRYING_TAIL_HEADS`].
fn tail_forms<'a>(
    view: &'a ExpressionView,
    context: &'a ExpressionView,
    depth: usize,
    out: &mut Vec<Tail<'a>>,
) {
    if depth >= MAX_TAIL_DEPTH {
        out.push(Tail {
            value: view,
            context,
        });
        return;
    }
    if head_is(view, &["if", "if-not"]) {
        // `(if test then else)`: the branches, not the test. A branch receives
        // nothing from the test, so each is its own context.
        let mut any = false;
        for branch in view.children.iter().skip(2) {
            any = true;
            tail_forms(branch, branch, depth + 1, out);
        }
        if !any {
            out.push(Tail {
                value: view,
                context,
            });
        }
        return;
    }
    if head_is(view, TAIL_POSITION_HEADS) {
        match view.children.last() {
            // A binding-taking form with an empty body — `(let [x 1])` — has
            // its binding vector as the last child, which is not a tail.
            //
            // There is deliberately no `!is_vector_literal(last)` guard here.
            // One was written, and mutation testing showed it could not change
            // a finding: the only thing done with a tail's `value` is
            // `is_lazy_sequence_call`, which goes through `list_head`, which is
            // `is_paren_list`-gated — so a `[…]` node yields `None` and is
            // never lazy. The guard was unreachable, and the behaviour it was
            // meant to protect is pinned by
            // `does_not_read_a_binding_vector_as_the_tail_of_a_body_less_form`.
            Some(last) if view.children.len() >= 2 => {
                let inner = if head_is(view, VALUE_CARRYING_TAIL_HEADS) {
                    view
                } else {
                    last
                };
                tail_forms(last, inner, depth + 1, out);
            }
            _ => out.push(Tail {
                value: view,
                context,
            }),
        }
        return;
    }
    out.push(Tail {
        value: view,
        context,
    });
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_with_open(
    view: &ExpressionView,
    scope_count: &mut usize,
    violations: &mut Vec<WithOpenLazySeqItem>,
) {
    if !head_is(view, RESOURCE_SCOPE_HEADS) {
        return;
    }
    *scope_count += 1;

    let Some(bindings) = view.children.get(1).filter(|node| is_vector_literal(node)) else {
        return;
    };
    let Some(body_tail) = view.children.get(2..).and_then(<[_]>::last) else {
        return;
    };

    let mut tails = Vec::new();
    tail_forms(body_tail, body_tail, 0, &mut tails);
    // The laziness test is a head lookup; reading the bound names allocates a
    // `String` per binding. Almost every `with-open` in a correct file has an
    // eager tail, so the cheap test goes first and the allocation never
    // happens — the same "no allocation until a candidate exists" ordering the
    // rest of this package follows.
    if !tails.iter().any(|tail| is_lazy_sequence_call(tail.value)) {
        return;
    }
    let names = bound_names(bindings);
    if names.is_empty() {
        return;
    }
    for tail in tails {
        if !is_lazy_sequence_call(tail.value) {
            continue;
        }
        let Some(resource) = names
            .iter()
            .find(|name| reaches_symbol_unrealized(tail.context, std::slice::from_ref(name)))
        else {
            continue;
        };
        let Some(operation) = list_head(tail.value).map(normalized_symbol) else {
            continue;
        };
        violations.push(WithOpenLazySeqItem {
            span: tail.value.span,
            operation,
            resource: resource.clone(),
        });
    }
}

/// Collects every lazily-returned resource scope in one file, with the number
/// of `with-open` forms scanned as the denominator beside them.
pub fn build_with_open_returns_lazy_seq_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<WithOpenLazySeqItem>> {
    let mut scope_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_with_open(view, &mut scope_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("resource_scope_count", json!(scope_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::LAZY_SEQUENCE_HEADS;

    fn report(input: &str) -> FileFindings<WithOpenLazySeqItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_with_open_returns_lazy_seq_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<WithOpenLazySeqItem> {
        report(input).findings
    }

    fn operations(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.operation)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_a_map_over_line_seq_returned_from_the_scope() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (map parse (line-seq r)))"),
            vec!["map".to_owned()]
        );
    }

    #[test]
    fn flags_a_bare_line_seq_returned_from_the_scope() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (line-seq r))"),
            vec!["line-seq".to_owned()]
        );
    }

    #[test]
    fn flags_a_lazy_tail_of_a_threading_macro() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (->> (line-seq r) (map parse) (filter ok?)))"),
            vec!["filter".to_owned()]
        );
    }

    #[test]
    fn flags_a_lazy_tail_reached_through_a_let() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (let [ls (line-seq r)] (map parse ls)))"),
            vec!["map".to_owned()]
        );
    }

    #[test]
    fn flags_a_for_comprehension_over_the_resource() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (for [l (line-seq r)] (parse l)))"),
            vec!["for".to_owned()]
        );
    }

    /// Either branch can be the value, so both are followed. Reporting only
    /// the first would make the finding depend on which the author wrote first.
    #[test]
    fn flags_a_lazy_branch_of_an_if() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (if skip? [] (map parse (line-seq r))))"),
            vec!["map".to_owned()]
        );
        assert_eq!(
            operations("(with-open [r (io/reader p)] (if skip? (map parse (line-seq r)) []))"),
            vec!["map".to_owned()]
        );
    }

    #[test]
    fn names_the_binding_the_tail_reaches() {
        let items =
            violations("(with-open [w (io/writer q) r (io/reader p)] (map parse (line-seq r)))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].resource, "r");
    }

    // --- realistic, correct Clojure that must stay silent --------------------

    /// The single most common correct spelling. `->>`'s last stage is the
    /// value, and `into` realizes.
    #[test]
    fn does_not_flag_an_eagerly_realized_threading_tail() {
        assert!(
            violations("(with-open [r (io/reader p)] (->> (line-seq r) (map parse) (into [])))")
                .is_empty()
        );
        assert!(
            violations("(with-open [r (io/reader p)] (doall (map parse (line-seq r))))").is_empty()
        );
        assert!(
            violations("(with-open [r (io/reader p)] (reduce + (map count (line-seq r))))")
                .is_empty()
        );
        assert!(violations("(with-open [r (io/reader p)] (mapv parse (line-seq r)))").is_empty());
        assert!(violations("(with-open [r (io/reader p)] (count (line-seq r)))").is_empty());
    }

    /// A lazy sequence that never touches the resource realizes fine after the
    /// close.
    #[test]
    fn does_not_flag_a_lazy_tail_that_does_not_reach_the_resource() {
        assert!(violations("(with-open [r (io/reader p)] (map inc [1 2 3]))").is_empty());
        assert!(
            violations("(with-open [r (io/reader p)] (map parse (line-seq other)))").is_empty()
        );
    }

    /// Unrealized, but over data already in memory: correct code.
    #[test]
    fn does_not_flag_a_lazy_wrapper_around_realized_data() {
        assert!(
            violations("(with-open [r (io/reader p)] (map parse (doall (line-seq r))))").is_empty()
        );
        assert!(
            violations("(with-open [r (io/reader p)] (map parse (into [] (line-seq r))))")
                .is_empty()
        );
    }

    /// A threading macro can name the realizer as a **bare symbol**, and
    /// `(->> (line-seq r) doall (map parse))` is correct code: `doall` realizes
    /// everything before `map` wraps it.
    ///
    /// Found by mutation testing. `is_realization_barrier`'s second branch —
    /// the one that reads bare threading stages — could be deleted with the
    /// whole suite still green, because every existing test spelled the
    /// realizer as a call.
    #[test]
    fn does_not_flag_a_realizer_written_as_a_bare_threading_stage() {
        assert!(
            violations("(with-open [r (io/reader p)] (->> (line-seq r) doall (map parse)))")
                .is_empty()
        );
        assert!(
            violations("(with-open [r (io/reader p)] (->> (line-seq r) vec (map parse)))")
                .is_empty()
        );
    }

    /// The negative control for the above: a bare stage that realizes nothing
    /// must leave the defect reportable.
    #[test]
    fn a_bare_threading_stage_that_is_not_a_realizer_does_not_silence_the_rule() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (->> (line-seq r) rest (map parse)))"),
            vec!["map".to_owned()]
        );
    }

    /// `with-open` requires a binding **vector**; a list is malformed. Reading
    /// its elements as bindings would name a resource that was never bound.
    ///
    /// Found by mutation testing: the `is_vector_literal` filter on the
    /// bindings could be deleted with the suite green.
    #[test]
    fn does_not_flag_a_scope_whose_bindings_are_not_a_vector() {
        assert!(violations("(with-open (r (io/reader p)) (map parse (line-seq r)))").is_empty());
        assert!(violations("(with-open r (map parse (line-seq r)))").is_empty());
    }

    /// A `reduce` inside the mapping function realizes nothing about the
    /// reader, so the barrier must not fire on it.
    #[test]
    fn does_not_let_a_sibling_realizer_hide_the_defect() {
        assert_eq!(
            operations("(with-open [r (io/reader p)] (map #(reduce + %) (line-seq r)))"),
            vec!["map".to_owned()]
        );
    }

    #[test]
    fn does_not_flag_a_side_effecting_scope() {
        assert!(violations("(with-open [w (io/writer p)] (doseq [x xs] (.write w x)))").is_empty());
        assert!(violations("(with-open [r (io/reader p)] (slurp r))").is_empty());
    }

    /// `doto` returns its *first* argument, so it is not a tail-position form
    /// and its last child must not be read as the value.
    #[test]
    fn does_not_read_the_last_child_of_a_form_that_returns_its_first() {
        assert!(
            violations("(with-open [r (io/reader p)] (doto acc (conj (map parse (line-seq r)))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_scope_with_no_body_or_no_bindings() {
        assert!(violations("(with-open [r (io/reader p)])").is_empty());
        assert!(violations("(with-open)").is_empty());
        assert!(violations("(with-open [] (map parse xs))").is_empty());
    }

    /// A binding-taking form with an **empty body** has its binding vector as
    /// its last child. That vector is not a tail, and reading it as one
    /// analyses the wrong node — with a bound name in head position standing in
    /// for the operation.
    ///
    /// `rest` and `next` are both in the laziness vocabulary *and* ordinary
    /// binding names, so `(let [rest …])` is all it takes: without the guard
    /// the rule reports `rest` as the lazy operation of a form that has no
    /// tail at all. Found by mutation testing.
    #[test]
    fn does_not_read_a_binding_vector_as_the_tail_of_a_body_less_form() {
        assert!(violations("(with-open [r (io/reader p)] (let [rest (line-seq r)]))").is_empty());
        assert!(
            violations("(with-open [r (io/reader p)] (when-let [next (line-seq r)]))").is_empty()
        );
    }

    /// A destructured binding names nothing this rule can compare, so it
    /// declines rather than guessing.
    #[test]
    fn does_not_flag_a_destructured_binding() {
        assert!(
            violations("(with-open [{:keys [in]} (open p)] (map parse (line-seq in)))").is_empty()
        );
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(with-open [r (io/reader p)] (map parse (line-seq r)))").is_empty());
        assert!(
            violations("(quote (with-open [r (io/reader p)] (map parse (line-seq r))))").is_empty()
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(with-open [r (io/reader p)] (map parse (line-seq r)))").is_empty());
    }

    /// Clojure reads `,` as whitespace, so `` `(a ,x) `` is all data.
    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(
            violations("'(a ,(with-open [r (io/reader p)] (map parse (line-seq r))))").is_empty()
        );
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            operations("`(do ~(with-open [r (io/reader p)] (map parse (line-seq r))))"),
            vec!["map".to_owned()]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(
            violations("(println \"(with-open [r (io/reader p)] (map parse (line-seq r)))\")")
                .is_empty()
        );
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_resource_scope_scanned() {
        let report = report(
            "(with-open [r (io/reader a)] (doall (line-seq r)))\n\
             (with-open [r (io/reader b)] (map parse (line-seq r)))\n",
        );
        assert_eq!(report.summary, vec![("resource_scope_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_operation_and_resource() {
        let report =
            report("(defn lines [p]\n  (with-open [r (io/reader p)]\n    (line-seq r)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "with-open-returns-lazy-seq");
        assert_eq!(
            finding.json_fields(),
            vec![("operation", json!("line-seq")), ("resource", json!("r"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["operation=line-seq".to_owned(), "resource=r".to_owned()]
        );
        assert!(finding.message().contains("realized too late"));
    }

    /// A Clojure-only rule must say "nothing was looked for" rather than
    /// "nothing was found" when handed another dialect.
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(with-open-file (r p) (mapcar #'parse (read-lines r)))",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let report = build_with_open_returns_lazy_seq_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("resource_scope_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(with-open [r (io/reader p)] (doall (line-seq r)))").dialect_modelled);
    }

    /// The anchor is the *scope*, not the laziness vocabulary: `map` is on
    /// nearly every line of Clojure, and a rule keyed on it would run its body
    /// constantly. See the head list on `rule.rs`.
    #[test]
    fn the_lazy_vocabulary_is_not_the_head_filter() {
        assert!(LAZY_SEQUENCE_HEADS.contains(&"map"));
        assert!(!LAZY_SEQUENCE_HEADS.contains(&"with-open"));
        assert_eq!(RESOURCE_SCOPE_HEADS, &["with-open"]);
    }
}
