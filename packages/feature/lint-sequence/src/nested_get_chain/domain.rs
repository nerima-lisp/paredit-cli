//! Clojure nested-`get` detection: a `(get (get m :a) :b)`, which is
//! `(get-in m [:a :b])`.
//!
//! `clojure.core/get-in` is defined as `(reduce1 get m ks)` in its two-argument
//! arity, so `(get-in m [:a :b])` *is* `(get (get m :a) :b)` — the same
//! successive lookups, in the same order, with the same nil-punning at every
//! step. The path spelled as a vector reads as a path; the same path spelled as
//! nesting has to be read inside out.
//!
//! # Only the two-operand `get`, at every level
//!
//! `get` has a three-operand arity, `(get m k not-found)`, and `get-in` has one
//! too, but they are not the same shape: `get-in`'s `not-found` applies to the
//! path as a whole, while a `not-found` on an inner `get` applies to that step
//! only. Rather than reason about when the two coincide, a `get` carrying a
//! `not-found` is not a chain link at all — it neither reports nor extends a
//! chain. What is reported is the maximal run of two-operand `get`s, which is
//! the case where the rewrite is `reduce1 get` by definition and needs no
//! third argument.
//!
//! A chain can therefore sit *inside* a three-operand `get`:
//! `(get (get (get m :a) :b) :c :missing)` reports its inner two-`get` run,
//! which is `(get-in m [:a :b])`, and says nothing about the outer lookup. That
//! is a real and safe rewrite, so suppressing it would be a false negative
//! rather than caution.
//!
//! # One finding per chain
//!
//! `(get (get (get m :a) :b) :c)` contains two nodes whose target is itself a
//! `get`, and the dispatcher visits both. Reporting both would be two findings
//! for one expression, so only the *outermost* `get` of a chain is reported:
//! the report path filters out any candidate that is another candidate's inner
//! `get`, and the rule path asks [`crate::support::enclosing_form`] whether its
//! own node is the target of an enclosing chain link. The two arrive at the
//! same answer by different routes, which is what
//! `the_two_suppression_routes_agree` pins.
//!
//! # What is *not* reported, and why
//!
//! - **A single `get`.** `(get m :a)` is not a path.
//! - **A `get` on a computed target.** `(get (find-map id) :a)` is one lookup;
//!   only a target that is itself a `get` is a chain link.
//! - **A reader-conditional operand**, `#?(:clj …)`, which has no settled shape.
//! - **`get` reached through a namespace**, `clojure.core/get`: the head index
//!   does not fold namespace qualifiers for Clojure, so the qualified spelling
//!   is a false negative rather than a match.
//!
//! Report-only: `get-in` allocates a vector for the path and is a different
//! function with its own cost, so which spelling belongs in a hot path is a
//! decision about the program.
//!
//! Scope: Clojure only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::for_each_evaluated_subview;

/// How many `get`s a chain needs before it is reported.
///
/// Two: `(get (get m :a) :b)` is the shortest expression `get-in` replaces, and
/// is exactly what its two-argument arity reduces to.
pub const MIN_CHAIN_LENGTH: usize = 2;

#[derive(Debug, Clone)]
pub struct GetChainItem {
    /// The span of the whole outermost `(get (get …) :k)` form.
    pub span: ByteSpan,
    /// The span of the inner `get`, which is this one's target.
    pub inner_span: ByteSpan,
    /// The span of the expression the whole path starts from.
    pub root_span: ByteSpan,
    /// How many `get`s the chain has.
    pub depth: usize,
}

impl Finding for GetChainItem {
    fn kind(&self) -> &'static str {
        "nested-get-chain"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.depth.to_string()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("depth", json!(self.depth)),
            ("root_span", span_json(self.root_span)),
        ]
    }

    fn message(&self) -> String {
        message_for(self.depth)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(depth: usize) -> String {
    format!(
        "{depth} nested gets read one path; get-in names the path as a vector \
         ((get (get m :a) :b) is (get-in m [:a :b]))"
    )
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// A reader conditional (`#?(:clj …)`), which is build-dependent and leaves the
/// form without a settled shape.
fn has_reader_conditional(view: &ExpressionView) -> bool {
    view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional | ReaderPrefix::ReaderConditionalSplicing
        )
    })
}

/// Whether `view` is a two-operand `(get target key)`, the only shape that
/// links a chain.
///
/// Public because the lint rule's suppression asks it about the *enclosing*
/// form, and both routes have to mean the same thing by "chain link" or a
/// nested `get` would be reported twice by one route and never by the other.
#[must_use]
pub fn is_chain_link(view: &ExpressionView) -> bool {
    is_paren_list(view)
        && view.children.len() == 3
        && list_head(view).is_some_and(|head| symbol_in(head, &["get"]))
        && !has_reader_conditional(view)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Emits a candidate for *every* chain link whose target is a chain link, which
/// for a three-deep chain is two candidates; suppressing the inner one is the
/// caller's job, because the two callers can see different things — see this
/// module's documentation.
///
/// # Cost
///
/// One `children.len()` and one `is_paren_list` reject every ordinary
/// `(get m :k)` before anything walks. `get` is a common head in Clojure, which
/// is why the target's shape is the second thing tested and the chain loop only
/// ever runs for a `get` that already has a `get` inside it.
pub fn examine(
    view: &ExpressionView,
    get_form_count: &mut usize,
    violations: &mut Vec<GetChainItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["get"]) {
        return;
    }
    *get_form_count += 1;

    if !is_chain_link(view) {
        return;
    }

    let mut depth = 1;
    let mut current = view;
    while is_chain_link(&current.children[1]) {
        depth += 1;
        current = &current.children[1];
    }
    if depth < MIN_CHAIN_LENGTH {
        return;
    }

    // The bottom of the chain is the innermost `get`'s own target.
    let root = &current.children[1];
    if has_reader_conditional(root) {
        return;
    }

    violations.push(GetChainItem {
        span: view.span,
        inner_span: view.children[1].span,
        root_span: root.span,
        depth,
    });
}

/// Drops every candidate that is another candidate's inner `get`, leaving one
/// finding per chain — the outermost.
fn only_outermost(candidates: Vec<GetChainItem>) -> Vec<GetChainItem> {
    let inner: Vec<ByteSpan> = candidates.iter().map(|item| item.inner_span).collect();
    candidates
        .into_iter()
        .filter(|item| !inner.contains(&item.span))
        .collect()
}

/// Collects every nested `get` chain in one file, with the number of `get`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no nested get chains here" for Clojure
/// and "nothing was looked for" for Common Lisp, and the two read identically
/// without the flag.
pub fn collect_get_chains(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<GetChainItem>> {
    if dialect != Dialect::Clojure {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("get_form_count", json!(0))],
        ));
    }

    let mut get_form_count = 0;
    let mut candidates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut get_form_count, &mut candidates);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        only_outermost(candidates),
        vec![("get_form_count", json!(get_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::enclosing_form;
    use paredit_core_syntax::view_query::for_each_subview;

    fn report(input: &str) -> FileFindings<GetChainItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        collect_get_chains(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect get chains")
    }

    /// Every candidate `examine` emits, before any suppression — which is what
    /// the dispatcher hands the rule, one node at a time.
    fn candidates(input: &str) -> Vec<GetChainItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        let mut count = 0;
        let mut found = Vec::new();
        for_each_subview(&tree.root_view(), |view| {
            examine(view, &mut count, &mut found);
        });
        found
    }

    fn depths(input: &str) -> Vec<usize> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.depth)
            .collect()
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_two_deep_get_chain() {
        let source = "(get (get m :a) :b)";
        let findings = report(source).findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].depth, 2);
        assert_eq!(slice(source, findings[0].root_span), "m");
        assert!(findings[0].message().contains("get-in"));
    }

    #[test]
    fn flags_a_three_deep_chain_once_at_its_outermost_get() {
        let source = "(get (get (get m :a) :b) :c)";
        let findings = report(source).findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].depth, 3);
        assert_eq!(slice(source, findings[0].span), source);
        assert_eq!(slice(source, findings[0].root_span), "m");
    }

    /// The dispatcher sees both chain links, so the rule must suppress one; the
    /// report filters the same pair down to the same single finding.
    #[test]
    fn the_two_suppression_routes_agree() {
        let source = "(get (get (get m :a) :b) :c)";
        assert_eq!(candidates(source).len(), 2);

        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
        let surviving: Vec<ByteSpan> = candidates(source)
            .into_iter()
            .filter(|item| {
                // Exactly the rule's own suppression.
                !enclosing_form(&tree, item.span).is_some_and(|parent| {
                    is_chain_link(&parent) && parent.children[1].span == item.span
                })
            })
            .map(|item| item.span)
            .collect();
        let reported: Vec<ByteSpan> = report(source)
            .findings
            .into_iter()
            .map(|item| item.span)
            .collect();
        assert_eq!(surviving, reported);
        assert_eq!(reported.len(), 1);
    }

    #[test]
    fn flags_a_chain_over_a_computed_root() {
        let source = "(get (get (find-doc id) :body) :title)";
        let findings = report(source).findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(slice(source, findings[0].root_span), "(find-doc id)");
    }

    #[test]
    fn flags_two_separate_chains_separately() {
        assert_eq!(
            depths("(f (get (get m :a) :b) (get (get n :c) :d))"),
            vec![2, 2]
        );
    }

    #[test]
    fn flags_a_chain_with_string_and_index_keys() {
        assert_eq!(depths("(get (get m \"a\") \"b\")"), vec![2]);
        assert_eq!(depths("(get (get grid 0) 1)"), vec![2]);
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_single_get() {
        assert!(depths("(get m :a)").is_empty());
    }

    #[test]
    fn does_not_flag_a_get_on_a_computed_target() {
        assert!(depths("(get (find-map id) :a)").is_empty());
        assert!(depths("(get (:body doc) :title)").is_empty());
    }

    /// `get-in`'s not-found applies to the whole path, an inner `get`'s to one
    /// step, so a `get` carrying one is not a chain link: it neither reports
    /// nor extends a chain.
    #[test]
    fn does_not_flag_a_chain_with_a_not_found_argument() {
        // The not-found is on the inner get, so there is no two-get run at all.
        assert!(depths("(get (get m :a :missing) :b)").is_empty());
        // The not-found is on the outer get, so the outer is not a link and the
        // inner is a lone `get`.
        assert!(depths("(get (get m :a) :b :missing)").is_empty());
    }

    /// A three-operand `get` does not report, but it does not hide a real
    /// chain inside itself either: the inner two-`get` run is `(get-in m [:a
    /// :b])` and rewriting it is safe, so suppressing it would be a false
    /// negative rather than caution.
    #[test]
    fn flags_a_two_get_run_nested_inside_a_not_found_get() {
        let source = "(get (get (get m :a) :b) :c :missing)";
        let findings = report(source).findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].depth, 2);
        assert_eq!(slice(source, findings[0].span), "(get (get m :a) :b)");
    }

    #[test]
    fn does_not_flag_a_get_in_that_is_already_written() {
        assert!(depths("(get-in m [:a :b])").is_empty());
    }

    #[test]
    fn does_not_flag_a_keyword_or_threading_access() {
        assert!(depths("(:b (:a m))").is_empty());
        assert!(depths("(-> m :a :b)").is_empty());
    }

    #[test]
    fn does_not_flag_a_wrong_arity_get() {
        assert!(depths("(get)").is_empty());
        assert!(depths("(get m)").is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_operand() {
        assert!(depths("(get (get #?(:clj m :cljs n) :a) :b)").is_empty());
    }

    // -- quoting and strings, through the report path ------------------------

    /// The five quote shapes, in the Clojure spellings: `~` is unquote and `,`
    /// is whitespace, so a test written with a comma would prove nothing.
    #[test]
    fn the_report_skips_the_five_quote_shapes() {
        for source in [
            "'(get (get m :a) :b)",
            "(quote (get (get m :a) :b))",
            "`(get (get m :a) :b)",
            "'(a ~(get (get m :a) :b))",
            "'(outer (get (get m :a) :b))",
        ] {
            assert!(
                report(source).findings.is_empty(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn an_unquote_inside_a_syntax_quote_is_code_again() {
        assert_eq!(report("`(a ~(get (get m :a) :b))").findings.len(), 1);
    }

    #[test]
    fn a_call_inside_a_string_literal_is_not_a_form() {
        assert!(
            report("(println \"(get (get m :a) :b)\")")
                .findings
                .is_empty()
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(get (get m :a) :b)", Dialect::CommonLisp)
            .expect("parse");
        let report =
            collect_get_chains(Path::new("t.lisp"), Dialect::CommonLisp, &tree).expect("collect");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_get_scanned_not_only_the_flagged_ones() {
        // Three `get`s: the two of the chain and one on its own.
        let report = report("(get (get m :a) :b)\n(get n :c)\n");
        assert_eq!(report.summary, vec![("get_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_root_span() {
        let report = report("(defn f [m]\n  (get (get m :a) :b))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "nested-get-chain");
        assert_eq!(finding.text_columns(), vec!["2".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("depth", json!(2)),
                ("root_span", span_json(finding.root_span)),
            ]
        );
    }
}
