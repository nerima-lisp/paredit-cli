//! A Clojure `future` or `promise` bound by `let` and then never mentioned
//! again.
//!
//! A future's value *and its exception* both live inside the future until
//! somebody dereferences it. `(let [f (future (risky))] (other-work))` runs
//! `risky` on a pool thread, and if it throws, nothing ever finds out: the
//! exception is stored, the future is dropped, and the program continues as if
//! the work had succeeded. A promise that is never read is the same shape from
//! the other end — a value produced for a consumer that does not exist.
//!
//! # What it looks at
//!
//! `let` and `loop` binding vectors only, and within them only a binding whose
//! init form is a literal `(future …)`, `(future-call …)`, `(promise)` or
//! `(delay …)` call. The verdict is then the narrowest one available: the bound
//! symbol **does not occur anywhere in the body at all**.
//!
//! That is deliberately much stricter than "is not dereferenced". `@f`,
//! `(deref f)`, `(realized? f)`, `(deliver p v)`, `(future-cancel f)` and
//! `(hand-off-to-someone f)` all mention the symbol, and all of them silence
//! this rule — including the ones that are not a dereference. A binding that is
//! passed somewhere might be awaited there, and this rule cannot follow it, so
//! it says nothing.
//!
//! "The body" is not the whole story either. `let` binds sequentially, so a
//! *later binding's init form* consumes the symbol just as the body does:
//!
//! ```clojure
//! (let [ready  (promise)
//!       server (start-server (assoc opts :on-ready (fn [] (deliver ready :ok))))
//!       _      (deref ready 10000 :timeout)]
//!   server)
//! ```
//!
//! uses `ready` twice and never in the body. Both regions are searched.
//!
//! What is left is a binding nothing in scope can possibly consume, which is
//! either dead code or a dropped result. Both are worth a sentence; neither is
//! ambiguous.
//!
//! # What it does not attempt
//!
//! - **Top-level `def`.** `(def p (promise))` is consumed from elsewhere in the
//!   program, and a single file cannot tell whether anyone reads it.
//! - **Deciding that a mention is not a read.** See above: any mention is
//!   enough.
//! - **Shadowing.** A later `let` that rebinds the same name counts as a
//!   mention of it, which errs silent.
//! - **Fire-and-forget written as an expression.** `(future (log-it))` with no
//!   binding at all is not this rule's subject — there is no binding to be
//!   unused, and discarding a future on purpose is a legitimate thing to write.
//!
//! Scope: Clojure only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, head_is, normalized_symbol, symbol_name};

/// The binding forms whose body is where a reference would have to appear.
pub const BINDING_HEADS: &[&str] = &["let", "loop", "if-let", "when-let", "binding"];

/// Constructors whose result exists to be dereferenced later.
///
/// `delay` is included because it has exactly the same "value stays inside
/// until somebody forces it" property, and `force`/`@` are how both are read.
const DEFERRED_HEADS: &[&str] = &["future", "future-call", "promise", "delay"];

#[derive(Debug, Clone)]
pub struct FuturePromiseNeverRealizedItem {
    /// The span of the binding's init form (`(future …)`).
    pub span: ByteSpan,
    /// The bound symbol, normalized.
    pub binding: String,
    /// Which constructor produced it, normalized.
    pub constructor: String,
}

impl Finding for FuturePromiseNeverRealizedItem {
    fn kind(&self) -> &'static str {
        "future-promise-never-realized"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("binding={}", self.binding),
            format!("constructor={}", self.constructor),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("binding", json!(self.binding)),
            ("constructor", json!(self.constructor)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} binds a {} that the body never mentions, so its value and any exception in it \
             are discarded",
            self.binding, self.constructor
        )
    }
}

/// Whether `name` occurs anywhere in `forms` as a symbol.
///
/// The reader prefix is stripped before the comparison, so `@result` counts as
/// a mention of `result` — which is the common case and the one that matters
/// most.
fn mentions(forms: &[ExpressionView], name: &str) -> bool {
    let mut found = false;
    for form in forms {
        for_each_evaluated_subview(form, |node| {
            if node.kind != ExpressionKind::Atom {
                return;
            }
            found = found || symbol_name(node).is_some_and(|symbol| symbol == name);
        });
        if found {
            return true;
        }
    }
    false
}

pub fn examine_binding(
    view: &ExpressionView,
    binding_count: &mut usize,
    violations: &mut Vec<FuturePromiseNeverRealizedItem>,
) {
    if !head_is(view, BINDING_HEADS) {
        return;
    }
    let Some(bindings) = view.children.get(1) else {
        return;
    };
    // Clojure binds in a `[…]` vector. A `(let ((x 1)) …)` here would be Common
    // Lisp, which this rule does not model.
    if bindings.delimiter != Some(Delimiter::Bracket) {
        return;
    }
    let Some(body) = view.children.get(2..) else {
        return;
    };

    for (index, pair) in bindings.children.chunks_exact(2).enumerate() {
        let [target, init] = pair else {
            continue;
        };
        let Some(constructor) = list_head(init)
            .map(normalized_symbol)
            .filter(|head| DEFERRED_HEADS.contains(&head.as_str()))
        else {
            continue;
        };
        *binding_count += 1;
        // Destructuring binds several names at once; none of them is the
        // deferred value itself, so there is nothing to say about it.
        let Some(name) = symbol_name(target) else {
            continue;
        };
        // `let` binds sequentially, so a later binding's init form is as much a
        // consumer as the body is: `(let [config (delay …) handler (make-handler
        // config)] …)` uses `config`, and so does the `_ (deref ready …)` idiom.
        let later_bindings = bindings.children.get((index + 1) * 2..).unwrap_or(&[]);
        if name == "_" || mentions(body, &name) || mentions(later_bindings, &name) {
            continue;
        }
        violations.push(FuturePromiseNeverRealizedItem {
            span: init.span,
            binding: name,
            constructor,
        });
    }
}

/// Collects every never-mentioned deferred binding in one file, with the number
/// of deferred bindings scanned as the denominator beside them.
pub fn build_future_promise_never_realized_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FuturePromiseNeverRealizedItem>> {
    let mut binding_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_binding(view, &mut binding_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("deferred_binding_count", json!(binding_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<FuturePromiseNeverRealizedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_future_promise_never_realized_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<FuturePromiseNeverRealizedItem> {
        report(input).findings
    }

    fn bindings(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.binding)
            .collect()
    }

    #[test]
    fn flags_a_future_the_body_never_mentions() {
        assert_eq!(
            bindings("(let [f (future (risky))] (other-work))"),
            vec!["f".to_owned()]
        );
    }

    #[test]
    fn flags_a_promise_the_body_never_mentions() {
        assert_eq!(
            bindings("(let [p (promise)] (start-worker))"),
            vec!["p".to_owned()]
        );
    }

    #[test]
    fn flags_a_delay_and_a_future_call_too() {
        assert_eq!(
            bindings("(let [d (delay (compute))] (log))"),
            vec!["d".to_owned()]
        );
        assert_eq!(
            bindings("(let [f (future-call worker)] (log))"),
            vec!["f".to_owned()]
        );
    }

    #[test]
    fn flags_only_the_unmentioned_one_of_several_bindings() {
        assert_eq!(
            bindings("(let [a (future (x)) b (future (y))] (deref a))"),
            vec!["b".to_owned()]
        );
    }

    #[test]
    fn records_which_constructor_produced_it() {
        let found = violations("(let [p (promise)] (log))");
        assert_eq!(found[0].constructor, "promise");
    }

    // --- realistic, correct Clojure that must stay silent ------------------

    #[test]
    fn does_not_flag_a_future_that_is_dereferenced() {
        assert!(violations("(let [f (future (work))] @f)").is_empty());
        assert!(violations("(let [f (future (work))] (deref f))").is_empty());
        assert!(violations("(let [f (future (work))] (deref f 1000 :timeout))").is_empty());
    }

    #[test]
    fn does_not_flag_a_promise_that_is_delivered_or_awaited() {
        assert!(violations("(let [p (promise)] (future (deliver p 1)) @p)").is_empty());
        assert!(violations("(let [p (promise)] (deliver p 1))").is_empty());
        assert!(violations("(let [p (promise)] (realized? p))").is_empty());
    }

    /// `let` binds sequentially: a later binding is a consumer.
    #[test]
    fn does_not_flag_a_binding_consumed_by_a_later_binding() {
        assert!(
            violations(
                "(let [config (delay (load-config)) handler (make-handler config)] \
                 (run-jetty handler {:port 8080}))"
            )
            .is_empty()
        );
        assert!(
            violations(
                "(let [ready (promise) \
                 server (start-server (assoc opts :on-ready (fn [] (deliver ready :ok)))) \
                 _ (deref ready 10000 :timeout)] server)"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_binding_that_is_handed_to_something_else() {
        assert!(violations("(let [f (future (work))] (register! f))").is_empty());
        assert!(violations("(let [f (future (work))] (future-cancel f))").is_empty());
    }

    #[test]
    fn does_not_flag_a_mention_nested_deep_in_the_body() {
        assert!(
            violations("(let [f (future (work))] (when ready? (do (log) (println @f))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_deferred_value_that_is_the_bodys_result() {
        assert!(violations("(let [f (future (work))] f)").is_empty());
    }

    #[test]
    fn does_not_flag_a_binding_of_something_that_is_not_deferred() {
        assert!(violations("(let [a (atom 0)] (log))").is_empty());
        assert!(violations("(let [x (compute)] (log))").is_empty());
    }

    #[test]
    fn does_not_flag_a_future_written_as_an_expression_with_no_binding() {
        assert!(violations("(do (future (log-it)) (carry-on))").is_empty());
    }

    #[test]
    fn does_not_flag_the_underscore_placeholder() {
        assert!(violations("(let [_ (future (fire-and-forget))] (carry-on))").is_empty());
    }

    #[test]
    fn does_not_flag_a_common_lisp_style_binding_list() {
        // `(let ((f (future …))) …)` is not Clojure's shape; the binding
        // vector test is what keeps this rule off it.
        assert!(violations("(let ((f (future (work)))) (log))").is_empty());
        // The shape that actually needs the guard: a `(…)` binding list whose
        // elements happen to pair up as Clojure's flat vector would. Without
        // the delimiter test this reads as `f` bound to `(future (work))`.
        assert!(violations("(let (f (future (work))) (log))").is_empty());
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(let [f (future (work))] (log))").is_empty());
        assert!(violations("(quote (let [f (future (work))] (log)))").is_empty());
    }

    /// Clojure reads `,` as whitespace, so both of these are plain data.
    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(violations("'(a ,(let [f (future (work))] (log)))").is_empty());
        assert!(violations("`(a ,(let [f (future (work))] (log)))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(let [f (future (work))] (log))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            bindings("`(do ~(let [f (future (work))] (log)))"),
            vec!["f".to_owned()]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(println \"(let [f (future (work))] (log))\")").is_empty());
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn the_summary_counts_every_deferred_binding_scanned() {
        let report = report("(let [a (future (x)) b (future (y))] (deref a))");
        assert_eq!(report.summary, vec![("deferred_binding_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_binding_and_constructor() {
        let report = report("(defn go []\n  (let [f (future (work))] (log)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "future-promise-never-realized");
        assert_eq!(
            finding.json_fields(),
            vec![("binding", json!("f")), ("constructor", json!("future"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["binding=f".to_owned(), "constructor=future".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "f binds a future that the body never mentions, so its value and any exception in \
             it are discarded"
        );
    }

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let ((f (future (work)))) 1)", Dialect::CommonLisp)
                .expect("parse");
        let report = build_future_promise_never_realized_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("deferred_binding_count", json!(0))]);
    }
}
