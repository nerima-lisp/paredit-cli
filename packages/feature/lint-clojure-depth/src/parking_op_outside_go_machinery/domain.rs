//! A `core.async` **parking** operation the `go` transform cannot reach.
//!
//! ```clojure
//! (go (doseq [c chs] (>! c v)))    ; the go body: transformed
//! (go (run! #(>! % v) chs))        ; inside an fn: not transformed
//! ```
//!
//! # The premise, read off `clojure/core.async`
//!
//! `<!`, `>!` and `alts!` are not functions that do anything. Each is a `defn`
//! whose **entire body is an assertion that it was rewritten away**
//! (`async.clj`:174-178, 213-218, 358-382):
//!
//! ```clojure
//! (defn <! [port] (assert nil "<! used not in (go ...) block"))
//! (defn >! [port val] (assert nil ">! used not in (go ...) block"))
//! (defn alts! [ports & {:as opts}] (assert nil "alts! used not in (go ...) block"))
//! ```
//!
//! They exist as a marker for the `go` macro's state-machine transform to
//! find. `go` expands through `go-impl` (`go.clj`:1044-1059), which builds a
//! state machine out of *the body it is handed* — and a `fn*` is a separate
//! function object, so nothing inside one is part of that state machine. A
//! parking op that survives the transform is therefore an `AssertionError` the
//! first time that code runs, or — with `*assert*` false at compile time —
//! silently `nil`, which is worse.
//!
//! `(alt! …)` (`async.clj`:429) is the macro over `alts!` and carries the same
//! docstring requirement, "Must be called inside a (go ...) block".
//!
//! # What it looks at
//!
//! `<!`, `>!`, `alts!` and `alt!` occurring inside a `go`/`go-loop` body
//! **behind a thread boundary** ([`crate::support::THREAD_BOUNDARY_HEADS`]) —
//! the mirror image of [`crate::go_block_blocking_channel_op`], which reports
//! the blocking ops *in front of* the same boundary. One walk, two sides.
//!
//! The boundary list is the whole rule, and each entry earns its place by a
//! macroexpansion that puts an `fn*` around the body. The asymmetry worth
//! stating twice is `for` against `doseq`:
//!
//! ```clojure
//! (go (doseq [x xs] (>! c x)))   ; correct: doseq is loop/recur, no fn*
//! (go (for   [x xs] (<! x)))     ; broken:  for builds a lazy seq through fn*
//! ```
//!
//! `doseq` expands to nested `loop`/`recur` with no `fn*` anywhere
//! (`core.clj`:3240-3290); `for`, `lazy-seq`, `delay`, `dosync` and `thread`
//! all wrap their bodies in one.
//!
//! # What it does not attempt
//!
//! - **A parking op with no enclosing `go` at all** — `(defn take-one [c] (<!
//!   c))`. It is the same runtime assertion and arguably the more common
//!   mistake, but finding it means asking a `<!` node about its ancestors,
//!   which costs the enclosing top-level form *per candidate* and is the
//!   quadratic shape this package refuses. Anchoring on `go` bounds the work
//!   by the block, and every `go` block is walked exactly once.
//! - **An alias-qualified `go`.** See the head-normalization gap in
//!   [`crate::support`].
//! - **`letfn`.** Its *body* is in the enclosing scope, so calling the whole
//!   form a boundary would report a parking op that is fine. A parking op
//!   inside one of its function definitions is a false negative.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    GO_HEADS, PARKING_CHANNEL_OPS, for_each_call_across_boundaries, for_each_evaluated_subview,
    is_go_block,
};

/// The heads [`examine_go_block_parking`] matches — the two IOC-block macros.
pub const GO_BLOCK_HEADS: &[&str] = GO_HEADS;

#[derive(Debug, Clone)]
pub struct ParkingOpOutsideGoMachineryItem {
    /// The span of the parking call itself, not of the enclosing `go`.
    pub span: ByteSpan,
    /// The parking operator, normalized.
    pub operator: String,
    /// The form that took it out of the state machine — a head, or `#()`.
    pub boundary: String,
    /// `go` or `go-loop`.
    pub block: String,
}

impl Finding for ParkingOpOutsideGoMachineryItem {
    fn kind(&self) -> &'static str {
        "parking-op-outside-go-machinery"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("boundary={}", self.boundary),
            format!("block={}", self.block),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("boundary", json!(self.boundary)),
            ("block", json!(self.block)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} is inside {}, which the {} state-machine transform does not rewrite; \
             it asserts \"{} used not in (go ...) block\" at run time",
            self.operator, self.boundary, self.block, self.operator
        )
    }
}

pub fn examine_go_block_parking(
    view: &ExpressionView,
    go_block_count: &mut usize,
    violations: &mut Vec<ParkingOpOutsideGoMachineryItem>,
) {
    let Some(block) = list_head(view).filter(|head| symbol_in(head, GO_BLOCK_HEADS)) else {
        return;
    };
    *go_block_count += 1;

    // Each child of the block is walked from a clean boundary state, which is
    // also how the `go` node itself avoids being mistaken for the nested block
    // the walk prunes at.
    for child in view.children.iter().skip(1) {
        for_each_call_across_boundaries(child, |node, head, crossed| {
            // A nested `go` re-opens the state machine: a parking op inside
            // `(go (thread (go (<! c))))` is fine, because the innermost `go`
            // transforms it. Pruning here is what expresses that, and it is
            // also what keeps the inner block's own findings from doubling.
            if is_go_block(node) {
                return false;
            }
            let Some(boundary) = crossed else {
                return true;
            };
            if symbol_in(head, PARKING_CHANNEL_OPS) {
                violations.push(ParkingOpOutsideGoMachineryItem {
                    span: node.span,
                    operator: head.to_owned(),
                    boundary: boundary.to_owned(),
                    block: block.to_owned(),
                });
            }
            true
        });
    }
}

/// Collects every unreachable parking operation in one file, with the number
/// of `go` blocks scanned as the denominator beside them.
pub fn build_parking_op_outside_go_machinery_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ParkingOpOutsideGoMachineryItem>> {
    let mut go_block_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_go_block_parking(view, &mut go_block_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("go_block_count", json!(go_block_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::READER_LAMBDA_NAME;

    fn report(input: &str) -> FileFindings<ParkingOpOutsideGoMachineryItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_parking_op_outside_go_machinery_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn operators(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    fn boundaries(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.boundary)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_each_parking_operation_behind_a_function_boundary() {
        assert_eq!(operators("(go (map (fn [c] (<! c)) chs))"), vec!["<!"]);
        assert_eq!(operators("(go (run! #(>! % v) chs))"), vec![">!"]);
        assert_eq!(
            operators("(go (map (fn [c] (alts! [c])) chs))"),
            vec!["alts!"]
        );
        assert_eq!(
            operators("(go (map (fn [c] (alt! c ([v] v))) chs))"),
            vec!["alt!"]
        );
    }

    /// The asymmetry the rule rests on. `doseq` is `loop`/`recur`; `for`
    /// builds a lazy sequence through an `fn*`.
    #[test]
    fn flags_a_parking_operation_in_a_for_but_not_in_a_doseq() {
        assert_eq!(operators("(go (for [c chs] (<! c)))"), vec!["<!"]);
        assert!(operators("(go (doseq [c chs] (>! c v)))").is_empty());
    }

    #[test]
    fn flags_a_parking_operation_behind_every_thread_boundary() {
        for (source, boundary) in [
            ("(go (thread (<! c)))", "thread"),
            ("(go (future (<! c)))", "future"),
            ("(go (delay (<! c)))", "delay"),
            ("(go (lazy-seq (cons (<! c) nil)))", "lazy-seq"),
            ("(go (dosync (<! c)))", "dosync"),
            ("(go (reify P (m [_] (<! c))))", "reify"),
            ("(go #(<! %))", READER_LAMBDA_NAME),
        ] {
            assert_eq!(boundaries(source), vec![boundary.to_owned()], "{source}");
        }
    }

    #[test]
    fn the_finding_names_the_outermost_boundary_not_the_innermost() {
        assert_eq!(
            boundaries("(go (thread (map #(<! %) chs)))"),
            vec!["thread".to_owned()]
        );
    }

    #[test]
    fn flags_a_parking_operation_in_a_go_loop() {
        assert_eq!(
            operators("(go-loop [] (map (fn [c] (<! c)) chs))"),
            vec!["<!"]
        );
        assert_eq!(
            report("(go-loop [] (map (fn [c] (<! c)) chs))").findings[0].block,
            "go-loop"
        );
    }

    // --- correct code that must stay silent ----------------------------------

    #[test]
    fn does_not_flag_a_parking_operation_in_the_go_body_itself() {
        for source in [
            "(go (<! c))",
            "(go (let [v (<! c)] v))",
            "(go (when ready? (>! c v)))",
            "(go (loop [] (when-some [v (<! c)] (recur))))",
            "(go (try (<! c) (catch Exception e nil)))",
            "(go (doseq [c chs] (>! c v)))",
            "(go-loop [] (when-some [v (<! in)] (>! out v) (recur)))",
            "(go (alt! c ([v] v) (timeout 100) :timeout))",
            // `letfn`'s body is in the enclosing scope.
            "(go (letfn [(g [] 1)] (<! c)))",
            "(go (locking o (<! c)))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_the_blocking_operations() {
        assert!(operators("(go (thread (<!! c)))").is_empty());
        assert!(operators("(go (future (>!! c v)))").is_empty());
    }

    /// A parking op inside a nested `go` is transformed by *that* `go`, which
    /// is the whole point of `(go (thread (go (<! c))))`.
    #[test]
    fn a_nested_go_re_opens_the_state_machine() {
        assert!(operators("(go (thread (go (<! c))))").is_empty());
        assert!(operators("(go (map (fn [c] (go (<! c))) chs))").is_empty());
        assert_eq!(
            report("(go (thread (go (<! c))))").summary,
            vec![("go_block_count", json!(2))]
        );
    }

    /// A parking op *outside* any `go` block is the same runtime assertion,
    /// and this rule declines it by design: finding it means an ancestor walk
    /// per candidate. Pinned so the false negative is a decision, not a bug.
    #[test]
    fn a_parking_operation_outside_every_go_block_is_a_documented_false_negative() {
        assert!(operators("(defn take-one [c] (<! c))").is_empty());
        assert!(operators("(<! c)").is_empty());
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(operators("'(go (fn [] (<! c)))").is_empty());
        assert!(operators("`(go (fn [] (<! c)))").is_empty());
        assert!(operators("(quote (go (fn [] (<! c))))").is_empty());
    }

    #[test]
    fn a_comment_body_is_never_flagged() {
        assert!(operators("(comment (go (fn [] (<! c))))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(operators("`(do ~(go (fn [] (<! c))))"), vec!["<!"]);
    }

    /// `#?(…)` is a paren list carrying a hash, exactly like `#(…)`. Treating
    /// it as a function body would report correct code — every branch of a
    /// reader conditional is still the `go` body.
    #[test]
    fn a_reader_conditional_is_not_a_function_boundary() {
        assert!(operators("(go #?(:clj (<! c) :cljs (<! c)))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(operators("(println \"(go (fn [] (<! c)))\")").is_empty());
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_go_block_scanned() {
        let report = report("(go (<! a))\n(go-loop [] (map #(<! %) chs))\n(defn f [] 1)\n");
        assert_eq!(report.summary, vec![("go_block_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_columns() {
        let report = report("(defn fan-in [chs out]\n  (go\n    (run! #(>! out (<! %)) chs)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "parking-op-outside-go-machinery");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!(">!")),
                ("boundary", json!("#()")),
                ("block", json!("go")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "operator=>!".to_owned(),
                "boundary=#()".to_owned(),
                "block=go".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            ">! is inside #(), which the go state-machine transform does not rewrite; \
             it asserts \">! used not in (go ...) block\" at run time"
        );
    }

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(go (a))", Dialect::CommonLisp).expect("parse");
        let report = build_parking_op_outside_go_machinery_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("go_block_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(go (<! c))").dialect_modelled);
    }
}
