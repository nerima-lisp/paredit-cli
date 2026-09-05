//! A `core.async` **blocking** channel operation reached from a `go` body.
//!
//! ```clojure
//! (go (>!! out (process (<! in))))   ; blocks a go-block thread
//! (go (>!  out (process (<! in))))   ; parks it
//! ```
//!
//! # The premise, read off `clojure/core.async`
//!
//! `go` blocks are multiplexed over a **fixed** thread pool. The `go` macro's
//! own docstring states the consequence directly (`async.clj`:493-497):
//!
//! > go blocks should not (either directly or indirectly) perform operations
//! > that may block indefinitely. Doing so risks depleting the fixed pool of
//! > go block threads, causing all go block processing to stop. This includes
//! > core.async blocking ops (those ending in !!) and other blocking IO.
//!
//! and `doc/reference.md`:120-124 says the same about the whole pool, then
//! adds the fact that decides whether this rule is worth having:
//!
//! > core.async includes a debugging facility to detect this situation (other
//! > kinds of blocking operation cannot be detected so this covers only part
//! > of the problem). To enable go checking, set the Java system property
//! > `clojure.core.async.go-checking=true`. This property is read once, at
//! > namespace load time, and should be used in development or testing, not
//! > in production.
//!
//! So core.async does ship a detector — and it is **runtime**, **off by
//! default**, and gated on a JVM property read once when `clojure.core.async`
//! itself loads. `defblockingop` (`async.clj`:150-159) is the whole of it:
//!
//! ```clojure
//! (defmacro defblockingop [op doc arglist & body]
//!   `(def ~op (if (Boolean/getBoolean "clojure.core.async.go-checking")
//!               (fn ~arglist (dispatch/check-blocking-in-dispatch) ~@body)
//!               (fn ~arglist ~@body))))
//! ```
//!
//! Nothing catches it statically. `go` expands through
//! `clojure.core.async.impl.go/go-impl` (`async.clj`:505-506, `go.clj`:1044-
//! 1059), which builds the state machine and never inspects the body for
//! blocking operations — so this is not the `loop`-macroexpansion case where
//! the compiler already rejects the shape and a lint rule would be worthless.
//!
//! # What it looks at
//!
//! The four `!!` operations — `<!!`, `>!!`, `alts!!`, `alt!!` — reached from a
//! `go`/`go-loop` body **without crossing a thread boundary**
//! ([`crate::support::THREAD_BOUNDARY_HEADS`]). The boundary test is the
//! rule's whole soundness:
//!
//! ```clojure
//! (go (thread (>!! out v)))   ; correct: `thread` runs on a real thread
//! (go (>!! out v))            ; the defect
//! ```
//!
//! `(thread …)` inside a `go` is the *documented repair* for this very
//! defect, so a rule that reported it would report the fix it recommends.
//!
//! # What it does not attempt
//!
//! - **Blocking IO that is not a channel op.** `(Thread/sleep 1000)`, a JDBC
//!   call, `@(future …)`, `(.get fut)` — all of them deplete the pool the same
//!   way, and none is syntactically distinguishable from an ordinary call.
//!   core.async's own documentation concedes the same limitation.
//! - **An alias-qualified `go`.** See the head-normalization gap in
//!   [`crate::support`].
//! - **Indirection.** `(go (helper))` where `helper` calls `<!!` is exactly
//!   the "or indirectly" case the docstring warns about, and it needs a call
//!   graph this rule does not have.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    GO_HEADS, for_each_call_across_boundaries, for_each_evaluated_subview, is_go_block,
};

/// The heads [`examine_go_block`] matches — the two IOC-block macros.
pub const GO_BLOCK_HEADS: &[&str] = GO_HEADS;

#[derive(Debug, Clone)]
pub struct GoBlockBlockingChannelOpItem {
    /// The span of the blocking call itself, not of the enclosing `go`.
    pub span: ByteSpan,
    /// The blocking operator, normalized.
    pub operator: String,
    /// The parking operator that belongs in a `go` body instead.
    pub parking: &'static str,
    /// `go` or `go-loop`.
    pub block: String,
}

impl Finding for GoBlockBlockingChannelOpItem {
    fn kind(&self) -> &'static str {
        "go-block-blocking-channel-op"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("parking={}", self.parking),
            format!("block={}", self.block),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("parking", json!(self.parking)),
            ("block", json!(self.block)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} blocks a thread from the fixed go-block pool inside this {}; \
             use the parking {}, or move the blocking call into (thread …)",
            self.operator, self.block, self.parking
        )
    }
}

/// The parking operator each blocking one is the `!!` spelling of.
///
/// One table rather than two lists, so the pairing cannot drift: the rule's
/// repair is "drop one `!`", and a blocking op with no parking twin would have
/// no repair to name.
pub const BLOCKING_TO_PARKING: &[(&str, &str)] = &[
    ("<!!", "<!"),
    (">!!", ">!"),
    ("alt!!", "alt!"),
    ("alts!!", "alts!"),
];

fn parking_twin(blocking: &str) -> Option<&'static str> {
    BLOCKING_TO_PARKING
        .iter()
        .find(|(name, _)| *name == blocking)
        .map(|(_, parking)| *parking)
}

pub fn examine_go_block(
    view: &ExpressionView,
    go_block_count: &mut usize,
    violations: &mut Vec<GoBlockBlockingChannelOpItem>,
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
            // A nested `go` is its own head match and reports its own
            // findings; descending would report them twice.
            if is_go_block(node) {
                return false;
            }
            if crossed.is_some() {
                return true;
            }
            if let Some(parking) = parking_twin(head) {
                violations.push(GoBlockBlockingChannelOpItem {
                    span: node.span,
                    operator: head.to_owned(),
                    parking,
                    block: block.to_owned(),
                });
            }
            true
        });
    }
}

/// Collects every blocking channel operation in a `go` body in one file, with
/// the number of `go` blocks scanned as the denominator beside them.
pub fn build_go_block_blocking_channel_op_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<GoBlockBlockingChannelOpItem>> {
    let mut go_block_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_go_block(view, &mut go_block_count, &mut violations);
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

    fn report(input: &str) -> FileFindings<GoBlockBlockingChannelOpItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_go_block_blocking_channel_op_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn operators(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_each_blocking_operation_directly_in_a_go_body() {
        assert_eq!(operators("(go (>!! c v))"), vec![">!!"]);
        assert_eq!(operators("(go (<!! c))"), vec!["<!!"]);
        assert_eq!(operators("(go (alts!! [c d]))"), vec!["alts!!"]);
        assert_eq!(operators("(go (alt!! c ([v] v)))"), vec!["alt!!"]);
    }

    #[test]
    fn flags_a_blocking_operation_in_a_go_loop() {
        assert_eq!(operators("(go-loop [] (>!! c 1) (recur))"), vec![">!!"]);
    }

    /// `doseq` expands to `loop`/`recur` with no `fn*`, so its body is still
    /// the go body — which is why the idiom works for parking ops and why a
    /// blocking op there is still the defect.
    #[test]
    fn flags_a_blocking_operation_under_the_ordinary_control_forms() {
        assert_eq!(operators("(go (doseq [x xs] (>!! c x)))"), vec![">!!"]);
        assert_eq!(operators("(go (when ready? (<!! c)))"), vec!["<!!"]);
        assert_eq!(operators("(go (let [v (<!! c)] v))"), vec!["<!!"]);
        assert_eq!(
            operators("(go (try (<!! c) (catch Exception e nil)))"),
            vec!["<!!"]
        );
        assert_eq!(operators("(go (loop [] (<!! c)))"), vec!["<!!"]);
    }

    #[test]
    fn reports_every_blocking_operation_in_one_block() {
        assert_eq!(operators("(go (>!! a 1) (<!! b))"), vec![">!!", "<!!"]);
    }

    #[test]
    fn the_finding_names_the_parking_operator_that_belongs_there() {
        let finding = &report("(go (>!! c v))").findings[0];
        assert_eq!(finding.parking, ">!");
        assert_eq!(finding.block, "go");
    }

    // --- correct code that must stay silent ----------------------------------

    /// The documented repair. Reporting it would report the fix this rule
    /// recommends.
    #[test]
    fn does_not_flag_a_blocking_operation_inside_a_thread() {
        assert!(operators("(go (thread (>!! c v)))").is_empty());
        assert!(operators("(go (io-thread (<!! c)))").is_empty());
        assert!(operators("(go (thread (loop [] (>!! c 1) (recur))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_blocking_operation_behind_any_other_thread_boundary() {
        for source in [
            "(go (future (>!! c v)))",
            "(go (let [f (fn [] (>!! c v))] f))",
            "(go (map #(>!! c %) xs))",
            "(go (delay (<!! c)))",
            "(go (lazy-seq (cons (<!! c) nil)))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_the_parking_operations() {
        assert!(operators("(go (>! c v))").is_empty());
        assert!(operators("(go (<! c))").is_empty());
        assert!(operators("(go (alts! [c d]))").is_empty());
        assert!(operators("(go (alt! c ([v] v)))").is_empty());
    }

    /// Outside a `go` block a blocking op is the correct operator, and this
    /// rule anchors on `go` precisely so it never says otherwise.
    #[test]
    fn does_not_flag_a_blocking_operation_outside_any_go_block() {
        assert!(operators("(defn drain [c] (loop [] (when (<!! c) (recur))))").is_empty());
        assert!(operators("(>!! c v)").is_empty());
    }

    /// A nested `go` is its own head match. Reporting through it would double
    /// every finding in it.
    #[test]
    fn a_nested_go_block_reports_its_own_findings_exactly_once() {
        assert_eq!(operators("(go (go (>!! c v)))"), vec![">!!"]);
        assert_eq!(
            report("(go (go (>!! c v)))").summary,
            vec![("go_block_count", json!(2))]
        );
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(operators("'(go (>!! c v))").is_empty());
        assert!(operators("(quote (go (>!! c v)))").is_empty());
        assert!(operators("`(go (>!! c v))").is_empty());
    }

    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(operators("'(a ,(go (>!! c v)))").is_empty());
        assert!(operators("`(a ,(go (>!! c v)))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(operators("`(do ~(go (>!! c v)))"), vec![">!!"]);
    }

    /// A `(comment …)` block is read and thrown away: it is where a Clojure
    /// author keeps scratch REPL forms, and `>!!` at a REPL is correct.
    #[test]
    fn a_comment_body_is_never_flagged() {
        assert!(operators("(comment (go (>!! c v)))").is_empty());
        assert_eq!(
            report("(comment (go (>!! c v)))").summary,
            vec![("go_block_count", json!(0))]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(operators("(println \"(go (>!! c v))\")").is_empty());
    }

    /// Every branch of a reader conditional is code the reader may select, and
    /// `#?(…)` is a paren list carrying a hash — the shape a naive reader-
    /// lambda test mistakes for a function body.
    #[test]
    fn a_reader_conditional_branch_is_still_the_go_body() {
        assert_eq!(
            operators("(go #?(:clj (>!! c v) :cljs (>! c v)))"),
            vec![">!!"]
        );
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_go_block_scanned() {
        let report = report("(go (>! a 1))\n(go-loop [] (>!! b 2))\n(defn f [] 1)\n");
        assert_eq!(report.summary, vec![("go_block_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_columns() {
        let report = report("(defn pump [in out]\n  (go\n    (>!! out (<! in))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "go-block-blocking-channel-op");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!(">!!")),
                ("parking", json!(">!")),
                ("block", json!("go")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "operator=>!!".to_owned(),
                "parking=>!".to_owned(),
                "block=go".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            ">!! blocks a thread from the fixed go-block pool inside this go; \
             use the parking >!, or move the blocking call into (thread …)"
        );
    }

    /// The table is the rule's repair; a blocking op with no parking twin
    /// would be reported with no fix to name, and one listed only in the table
    /// would never be reached.
    #[test]
    fn every_blocking_operation_has_exactly_one_parking_twin() {
        let from_table: Vec<&str> = BLOCKING_TO_PARKING
            .iter()
            .map(|(blocking, _)| *blocking)
            .collect();
        assert_eq!(from_table, crate::support::BLOCKING_CHANNEL_OPS);
        for (blocking, parking) in BLOCKING_TO_PARKING {
            assert_eq!(parking_twin(blocking), Some(*parking));
            assert_eq!(
                *blocking,
                format!("{parking}!"),
                "the parking twin is the blocking spelling with one ! removed"
            );
            assert!(crate::support::PARKING_CHANNEL_OPS.contains(parking));
        }
        assert_eq!(parking_twin("<!"), None);
        assert_eq!(parking_twin("put!"), None);
    }

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(go (a))", Dialect::CommonLisp).expect("parse");
        let report = build_go_block_blocking_channel_op_report(
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
