//! A Clojure `swap!` whose update function does something other than compute a
//! value.
//!
//! `swap!` is a compare-and-set loop. When another thread wins the race, the
//! function is called again — and again — until the swap lands. The contract is
//! therefore that the function must be *pure*: anything it does besides return
//! a new value happens an unpredictable number of times. A `(swap! log (fn [l]
//! (println "adding") (conj l x)))` prints once on a quiet system and three
//! times under contention, which is exactly the kind of bug that never
//! reproduces.
//!
//! # What it looks at
//!
//! The update function of `swap!`, `swap-vals!`, `alter` and `commute`, and
//! only when it is written inline as `(fn [x] …)`, `(fn name [x] …)` or `#(…)`.
//! A named function — `(swap! a inc)`, `(swap! a update :k inc)` — is a
//! reference this rule cannot follow, and is never flagged.
//!
//! Inside that function, a call is treated as a side effect when its head is
//! one of a short list of Clojure core operations that exist *for* their effect
//! (`println`, `spit`, `send`, `deliver`, `alter-var-root`, `set!`, …), or when
//! its head ends in `!` — the convention Clojure itself uses to mark the
//! operations that are not safe inside a transaction or a retry, which is the
//! same safety property `swap!` needs.
//!
//! # Three things that look like effects and are not
//!
//! - **Transients.** `assoc!`, `conj!`, `dissoc!`, `disj!`, `pop!`,
//!   `persistent!` and `transient` all carry the `!`, and all of them are
//!   retry-safe when the transient is built from the update's own immutable
//!   input — which is the only correct way to use them, and the way
//!   `clojure.core/into` itself is written.
//! - **`throw`.** It aborts the update rather than repeating inside it.
//!   Validate-then-throw in a `swap!` function is a standard shape.
//! - **An effect in a stored body.** A `fn`, `#(…)`, `delay` or `lazy-seq`
//!   written inside the update stores its body; the effect happens when
//!   somebody calls or forces it, not on every retry. Those subtrees are not
//!   descended into.
//!
//! # What it does not attempt
//!
//! - **Java interop.** `(.write w x)` is a side effect and is not flagged: a
//!   `.method` head would also match plenty of pure accessors, and this rule
//!   would rather miss it.
//! - **Effects behind a user-defined name.** `(swap! a (fn [x] (record! x)))`
//!   is caught only because of the `!`; `(swap! a (fn [x] (record x)))` is not.
//! - **Laziness.** A `map` over a side-effecting function inside the update is
//!   not unwound; only calls written in the function's own body are seen.
//! - **Whether the effect is idempotent.** Some are, and repeating them is
//!   harmless. The rule reports the shape and leaves that judgment to a reader.
//!
//! Scope: Clojure only. Common Lisp has no `swap!`, and the `!` convention
//! means nothing there.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, for_each_evaluated_subview_where, head_is, normalized_symbol,
};

/// The reference-updating operators whose function may be retried.
///
/// `swap!` and `swap-vals!` retry on CAS failure; `alter` and `commute` run
/// inside an STM transaction, which retries for the same reason. All four ask
/// for the same purity.
pub const RETRYING_UPDATE_HEADS: &[&str] = &["swap!", "swap-vals!", "alter", "commute"];

/// Clojure core operations that exist for their effect.
///
/// Short and specific on purpose: a long list of "probably impure" names is how
/// a rule like this starts reporting on correct code.
/// `throw` is deliberately absent: it *aborts* the update rather than repeating
/// inside it, and validate-then-throw is a standard shape in a `swap!` function.
const EFFECTFUL_HEADS: &[&str] = &[
    "println",
    "print",
    "prn",
    "printf",
    "pr",
    "spit",
    "slurp",
    "send",
    "send-off",
    "deliver",
    "future",
    "alter-var-root",
    "set!",
];

/// `!`-suffixed operations that are nonetheless safe to repeat.
///
/// The transient functions all take a transient built *from the update's own
/// immutable input*, so every retry rebuilds it from scratch and no state
/// escapes. `(swap! idx (fn [m] (persistent! (reduce conj! (transient m) xs))))`
/// is the textbook Clojure way to do a bulk update, and `clojure.core/into`
/// itself is written that way — flagging it would fire on idiomatic code.
const RETRY_SAFE_BANG_NAMES: &[&str] = &[
    "assoc!",
    "conj!",
    "dissoc!",
    "disj!",
    "pop!",
    "persistent!",
    "transient",
];

/// Forms whose body is *stored* rather than run, so an effect written inside
/// one does not happen during the update at all.
///
/// A handler registered by `(swap! handlers (fn [hs] (assoc hs k (fn [msg]
/// (println msg)))))` prints when the handler is later called, not when the
/// swap retries. Same for a `delay` that memoizes a lazy load.
const DEFERRED_BODY_HEADS: &[&str] = &["fn", "fn*", "delay", "lazy-seq", "future-call"];

#[derive(Debug, Clone)]
pub struct AtomSwapWithSideEffectItem {
    /// The span of the side-effecting call inside the update function.
    pub span: ByteSpan,
    /// The head of that call, normalized.
    pub call: String,
    /// The reference operator whose retry loop repeats it.
    pub operator: String,
}

impl Finding for AtomSwapWithSideEffectItem {
    fn kind(&self) -> &'static str {
        "atom-swap-with-side-effect"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("call={}", self.call),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("call", json!(self.call)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} retries its update function, so the {} call in it can run more than once",
            self.operator, self.call
        )
    }
}

/// Whether a call head names an operation that is there for its effect.
///
/// The `!` suffix is Clojure's own marker for "not safe to repeat" — it is what
/// `swap!`, `reset!`, `conj!` and `alter-var-root` are all spelled with — so a
/// call to one inside a retried function is the defect stated in the language's
/// own vocabulary.
fn is_effectful(head: &str) -> bool {
    let name = normalized_symbol(head);
    if RETRY_SAFE_BANG_NAMES.contains(&name.as_str()) {
        return false;
    }
    symbol_in(head, EFFECTFUL_HEADS) || name.len() > 1 && name.ends_with('!')
}

/// Whether a node's body is stored for later rather than run by the update.
fn defers_its_body(view: &ExpressionView) -> bool {
    head_is(view, DEFERRED_BODY_HEADS)
        || view
            .reader_prefixes
            .contains(&paredit_core_syntax::sexpr::ReaderPrefix::HashLiteral)
}

/// The body forms of an inline update function, or `None` when the update is a
/// reference rather than a literal.
///
/// Three literal spellings: `(fn [x] body…)`, `(fn name [x] body…)` and the
/// `#(…)` reader lambda, whose node is an ordinary paren list carrying a
/// `HashLiteral` reader prefix — so its whole content is the body.
fn inline_function_body(update: &ExpressionView) -> Option<&[ExpressionView]> {
    if !is_paren_list(update) {
        return None;
    }
    let Some(head) = list_head(update) else {
        // `#(…)` has no `fn` head; its first child is already part of the body.
        return Some(update.children.as_slice());
    };
    if !symbol_in(head, &["fn", "fn*"]) {
        // `#(inc %)` reads as a paren list headed by `inc`; the reader prefix
        // is what makes it a function literal.
        return update
            .reader_prefixes
            .contains(&paredit_core_syntax::sexpr::ReaderPrefix::HashLiteral)
            .then_some(update.children.as_slice());
    }
    // Skip `fn`, an optional name, and the parameter vector.
    let start = update
        .children
        .iter()
        .position(|child| child.delimiter == Some(paredit_core_syntax::sexpr::Delimiter::Bracket))
        .map_or(1, |index| index + 1);
    update.children.get(start..)
}

pub fn examine_swap(
    view: &ExpressionView,
    swap_count: &mut usize,
    violations: &mut Vec<AtomSwapWithSideEffectItem>,
) {
    if !head_is(view, RETRYING_UPDATE_HEADS) {
        return;
    }
    *swap_count += 1;

    let Some(operator) = list_head(view).map(normalized_symbol) else {
        return;
    };
    let Some(update) = view.children.get(2) else {
        return;
    };
    let Some(body) = inline_function_body(update) else {
        return;
    };

    for form in body {
        for_each_evaluated_subview_where(
            form,
            // A nested `fn`/`#(…)`/`delay` body is stored, not run: an effect
            // written inside one happens when somebody calls or forces it, not
            // on every retry of this update.
            |node| !defers_its_body(node),
            |node| {
                // `descend` is asked *after* the node is visited, so a
                // `#(println %)` — whose own head is the effectful call — has
                // to be skipped here as well as pruned there.
                if defers_its_body(node) {
                    return;
                }
                let Some(head) = list_head(node) else {
                    return;
                };
                if !is_effectful(head) {
                    return;
                }
                violations.push(AtomSwapWithSideEffectItem {
                    span: node.span,
                    call: normalized_symbol(head),
                    operator: operator.clone(),
                });
            },
        );
    }
}

/// Collects every side-effecting update function in one file, with the number
/// of retrying update forms scanned as the denominator beside them.
pub fn build_atom_swap_with_side_effect_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AtomSwapWithSideEffectItem>> {
    let mut swap_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_swap(view, &mut swap_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("retrying_update_count", json!(swap_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AtomSwapWithSideEffectItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_atom_swap_with_side_effect_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<AtomSwapWithSideEffectItem> {
        report(input).findings
    }

    fn calls(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.call)
            .collect()
    }

    #[test]
    fn flags_a_println_inside_a_swap_update() {
        assert_eq!(
            calls("(swap! log (fn [l] (println \"adding\") (conj l x)))"),
            vec!["println".to_owned()]
        );
    }

    #[test]
    fn flags_a_bang_named_call_inside_a_swap_update() {
        assert_eq!(
            calls("(swap! a (fn [x] (record-metric! x) (inc x)))"),
            vec!["record-metric!".to_owned()]
        );
    }

    #[test]
    fn flags_a_nested_swap_on_another_atom() {
        assert_eq!(
            calls("(swap! a (fn [x] (swap! b inc) x))"),
            vec!["swap!".to_owned()]
        );
    }

    #[test]
    fn flags_an_effect_inside_a_reader_lambda() {
        assert_eq!(
            calls("(swap! a #(do (println %) (inc %)))"),
            vec!["println".to_owned()]
        );
    }

    #[test]
    fn flags_an_effect_under_the_stm_operators_too() {
        assert_eq!(
            calls("(alter r (fn [x] (println x) x))"),
            vec!["println".to_owned()]
        );
        assert_eq!(
            calls("(commute r (fn [x] (spit \"f\" x) x))"),
            vec!["spit".to_owned()]
        );
    }

    #[test]
    fn flags_an_effect_nested_deep_in_the_update_body() {
        assert_eq!(
            calls("(swap! a (fn [x] (if (pos? x) (println x) nil)))"),
            vec!["println".to_owned()]
        );
    }

    // --- realistic, correct Clojure that must stay silent ------------------

    #[test]
    fn does_not_flag_a_pure_update() {
        assert!(violations("(swap! a (fn [x] (assoc x :k v)))").is_empty());
        assert!(violations("(swap! counter (fn [n] (inc n)))").is_empty());
        assert!(violations("(swap! a #(update % :n inc))").is_empty());
    }

    /// Building with transients inside the update is the textbook Clojure bulk
    /// update, and it is retry-safe: the transient is made from the update's own
    /// immutable input every time and never escapes.
    #[test]
    fn does_not_flag_transients_built_inside_the_update() {
        assert!(
            violations(
                "(swap! idx (fn [m] (persistent! (reduce (fn [a x] (assoc! a (:id x) x)) \
                 (transient m) xs))))"
            )
            .is_empty()
        );
        assert!(violations("(swap! s (fn [x] (persistent! (conj! (transient x) 1))))").is_empty());
    }

    /// A `throw` aborts the update; it cannot happen twice. Validate-then-throw
    /// inside a `swap!` function is standard.
    #[test]
    fn does_not_flag_a_validating_throw() {
        assert!(
            violations(
                "(swap! account (fn [a] (if (< (:balance a) amount) \
                 (throw (ex-info \"insufficient funds\" {})) (update a :balance - amount))))"
            )
            .is_empty()
        );
    }

    /// The effect is *stored*, not run: it happens when the handler is called
    /// or the delay is forced, not on every retry.
    #[test]
    fn does_not_flag_an_effect_inside_a_stored_closure_or_delay() {
        assert!(
            violations("(swap! handlers (fn [hs] (assoc hs topic (fn [msg] (println msg)))))")
                .is_empty()
        );
        assert!(
            violations("(swap! cache (fn [c] (assoc c k (delay (slurp \"conf.edn\")))))")
                .is_empty()
        );
        assert!(
            violations("(swap! hs (fn [h] (assoc h k #(println %))))").is_empty(),
            "a reader lambda stores its body too"
        );
    }

    #[test]
    fn does_not_flag_a_named_update_function() {
        assert!(violations("(swap! counter inc)").is_empty());
        assert!(violations("(swap! a update :k inc)").is_empty());
        assert!(violations("(swap! a my-pure-transform arg)").is_empty());
    }

    #[test]
    fn does_not_flag_a_named_fn_whose_body_is_pure() {
        assert!(violations("(swap! a (fn step [x] (conj x 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_an_effect_outside_any_retrying_operator() {
        assert!(violations("(let [f (fn [x] (println x))] (f 1))").is_empty());
    }

    /// `reset!` is not a retry loop — it writes once — so an effect beside it
    /// is not this rule's subject.
    #[test]
    fn does_not_flag_reset_which_does_not_retry() {
        assert!(violations("(reset! a (do (println 1) 2))").is_empty());
    }

    #[test]
    fn does_not_flag_a_single_character_bang() {
        // A head spelled `!` alone is not the `name!` convention.
        assert!(violations("(swap! a (fn [x] (! x)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_swap_with_no_update_function_at_all() {
        assert!(violations("(swap! a)").is_empty());
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(swap! a (fn [x] (println x)))").is_empty());
        assert!(violations("(quote (swap! a (fn [x] (println x))))").is_empty());
    }

    /// Clojure reads `,` as whitespace, so `` `(a ,x) `` is `` `(a x) `` — all
    /// data. The hard-quote case is the same shape with `'`.
    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(violations("'(a ,(swap! b (fn [x] (println x))))").is_empty());
        assert!(violations("`(a ,(swap! b (fn [x] (println x))))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(swap! a (fn [x] (println x)))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            calls("`(do ~(swap! a (fn [x] (println x))))"),
            vec!["println".to_owned()]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(println \"(swap! a (fn [x] (println x)))\")").is_empty());
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn the_summary_counts_every_retrying_update_scanned() {
        let report = report("(swap! a inc)\n(swap! b (fn [x] (println x)))\n");
        assert_eq!(report.summary, vec![("retrying_update_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_operator_and_call() {
        let report = report("(defn go []\n  (swap! log (fn [l] (println l) l)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "atom-swap-with-side-effect");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("swap!")), ("call", json!("println"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["operator=swap!".to_owned(), "call=println".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "swap! retries its update function, so the println call in it can run more than once"
        );
    }

    /// A Clojure-only rule must say "nothing was looked for" rather than
    /// "nothing was found" when handed Common Lisp.
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        // Spelled without a `[…]` parameter vector, which Common Lisp's reader
        // does not accept at all.
        let tree =
            SyntaxTree::parse_with_dialect("(swap! a (fn (x) (println x)))", Dialect::CommonLisp)
                .expect("parse");
        let report = build_atom_swap_with_side_effect_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("retrying_update_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(swap! a inc)").dialect_modelled);
    }
}
