//! Common Lisp redundant-accumulator detection: a `loop` whose only
//! accumulation is `collect … into acc` and whose only epilogue is
//! `finally (return acc)`.
//!
//! ```text
//! (loop for x in items collect (f x) into acc finally (return acc))
//! (loop for x in items collect (f x))                       ; the same thing
//! ```
//!
//! The `into` variable and the `finally` clause between them do exactly what
//! `loop`'s implicit accumulator already does. Verified against SBCL: the two
//! forms above return `equal` results.
//!
//! # The `named` trap
//!
//! `(loop named outer … finally (return acc))` is **not** the same shape, and
//! this rule refuses it. `loop named outer` establishes a block called `outer`
//! *instead of* the implicit `nil` block, so the `(return acc)` inside
//! `finally` returns from whatever enclosing `block nil` there is — not from
//! the loop. Verified against SBCL:
//!
//! ```text
//! (block nil (list :v (loop named outer for x in '(1 2) collect x into a
//!                         finally (return a))))
//!   => (1 2)          ; the block returned, the `list` never ran
//! (block nil (list :v (loop        for x in '(1 2) collect x into a
//!                         finally (return a))))
//!   => (:V (1 2))     ; the loop returned
//! ```
//!
//! Rewriting the first as a plain `collect` would change what the enclosing
//! code returns. The guard is not a nicety.
//!
//! # What this rule requires, all of it
//!
//! - Exactly one accumulation clause in the whole loop, and it is
//!   `collect`/`collecting`.
//! - Exactly one `into` in the loop, belonging to that clause, naming a plain
//!   symbol.
//! - Exactly one `finally`, and it is the last clause, and its whole body is
//!   the single form `(return acc)` naming that same symbol.
//! - The accumulator occurs exactly twice in the entire loop form: at `into`
//!   and at `return`. Anything else — `when (> (length acc) 3) do …`, a second
//!   `collect … into acc`, a `(print acc)` — means the variable is load-bearing
//!   and the rewrite is not equivalent.
//! - No `named`.
//!
//! # What this rule deliberately does not flag
//!
//! - **The other accumulation verbs.** `append`, `nconc`, `sum`, `count`,
//!   `maximize` and friends have the same theorem, but the rule's name and
//!   message are about `collect`, and a false negative costs nothing.
//! - **`finally (return-from …)`**, which is a deliberate non-local exit.
//! - **A `finally` with more than the one `return` form.** A trailing epilogue
//!   after a returning `finally` is
//!   `paredit-feature-lint-iteration-flow`'s `loop-unreachable-finally-clause`,
//!   a different complaint about a different shape; requiring the `finally`
//!   body to be exactly `(return acc)` keeps the two disjoint.
//! - **A form reached only as quoted data.**
//!
//! Report only: the rewrite deletes two clauses and is easy to state, but a
//! `loop`'s clause layout is hand-formatted and this project has a documented
//! history of autofixes corrupting source.
//!
//! Scope: Common Lisp only. `loop`'s extended syntax is Common Lisp's;
//! Emacs Lisp spells it `cl-loop` and Clojure has no such macro.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    atom_in, bindable_variable_name, count_symbol_occurrences, for_each_evaluated_subview,
    normalized_symbol, symbol_text,
};

/// Every `loop` accumulation verb, in the spelling clause keywords are compared
/// in.
///
/// Used only to *count* them — "is this loop accumulating more than one thing?"
/// — never to decide what to report, so a verb missing from this list could
/// only make the rule louder, and that is why the list is the full CLHS 6.1.3
/// set rather than the two the rule reports on.
const ACCUMULATION_VERBS: [&str; 14] = [
    "append",
    "appending",
    "collect",
    "collecting",
    "count",
    "counting",
    "maximize",
    "maximizing",
    "minimize",
    "minimizing",
    "nconc",
    "nconcing",
    "sum",
    "summing",
];

/// The two verbs this rule reports on.
const COLLECT_VERBS: [&str; 2] = ["collect", "collecting"];

#[derive(Debug, Clone)]
pub struct LoopCollectIntoImmediatelyReturnedItem {
    /// The span of the whole `(loop …)` form.
    pub span: ByteSpan,
    /// The accumulator variable, normalized.
    pub accumulator: String,
    /// The `collect` verb as the loop spells it, normalized.
    pub verb: String,
}

impl Finding for LoopCollectIntoImmediatelyReturnedItem {
    fn kind(&self) -> &'static str {
        "loop-collect-into-immediately-returned"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.accumulator.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("accumulator", json!(self.accumulator)),
            ("verb", json!(self.verb)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "loop {verb}s into {acc} only to return {acc} from finally; \
             drop `into {acc}` and the finally clause and let {verb} accumulate",
            verb = self.verb,
            acc = self.accumulator
        )
    }
}

/// Where each clause keyword this rule cares about sits, and how many times
/// each occurs, from one allocation-free pass over the loop's own children.
///
/// One pass rather than three, and no `Vec`: an earlier draft collected three
/// position lists and compared atoms through an owned, lowercased `String` per
/// atom, which measured 53× the package's cheapest rule. Only the loop form's
/// *own* children are read — a symbol nested inside a clause operand is not a
/// clause keyword.
#[derive(Default)]
struct ClauseIndex {
    named: bool,
    accumulation: Option<usize>,
    accumulation_count: usize,
    into: Option<usize>,
    into_count: usize,
    finally: Option<usize>,
    finally_count: usize,
}

fn index_clauses(view: &ExpressionView) -> ClauseIndex {
    let mut index = ClauseIndex::default();
    for (position, child) in view.children.iter().enumerate().skip(1) {
        if !child.reader_prefixes.is_empty() {
            continue;
        }
        let Some(text) = symbol_text(child) else {
            continue;
        };
        if text.eq_ignore_ascii_case("named") {
            index.named = true;
        } else if text.eq_ignore_ascii_case("into") {
            index.into.get_or_insert(position);
            index.into_count += 1;
        } else if text.eq_ignore_ascii_case("finally") {
            index.finally.get_or_insert(position);
            index.finally_count += 1;
        } else if ACCUMULATION_VERBS
            .iter()
            .any(|verb| text.eq_ignore_ascii_case(verb))
        {
            index.accumulation.get_or_insert(position);
            index.accumulation_count += 1;
        }
    }
    index
}

/// What one candidate loop was read as, before the occurrence check.
struct CollectIntoShape {
    accumulator: String,
    verb: String,
}

/// Reads the one shape this rule reports, or `None` for everything else.
///
/// Every early return is a documented guard; see the module doc.
fn read_collect_into_shape(view: &ExpressionView) -> Option<CollectIntoShape> {
    let clauses = index_clauses(view);

    // `named` moves the block the `finally`'s `return` leaves.
    if clauses.named {
        return None;
    }
    // Exactly one accumulation clause in the whole loop, exactly one `into`,
    // exactly one `finally`: two of any of them means the rewrite is not a
    // rewrite.
    if clauses.accumulation_count != 1 || clauses.into_count != 1 || clauses.finally_count != 1 {
        return None;
    }
    let verb_index = clauses.accumulation?;
    let into_index = clauses.into?;
    let finally_index = clauses.finally?;

    // `collect VALUE into ACC`: the three positions the grammar fixes.
    if !atom_in(&view.children[verb_index], &COLLECT_VERBS) {
        return None;
    }
    if into_index != verb_index + 2 {
        return None;
    }
    let accumulator = bindable_variable_name(view.children.get(into_index + 1)?)?;

    // `finally (return ACC)` and nothing after it: the finally clause must be
    // the whole epilogue, or dropping it changes what runs.
    if finally_index + 2 != view.children.len() {
        return None;
    }
    let epilogue = view.children.get(finally_index + 1)?;
    if !is_paren_list(epilogue) || !epilogue.reader_prefixes.is_empty() {
        return None;
    }
    // `is_none_or` rather than `!…is_some_and`: nix's clippy is newer than the
    // local one and rejects the negated form.
    if list_head(epilogue).is_none_or(|head| !symbol_in(head, &["return"])) {
        return None;
    }
    if epilogue.children.len() != 2 {
        return None;
    }
    if bindable_variable_name(&epilogue.children[1])? != accumulator {
        return None;
    }

    Some(CollectIntoShape {
        accumulator,
        verb: normalized_symbol(&view.children[verb_index])?,
    })
}

/// Examines one node. Shared with the lint suite's rule.
///
/// Cheapest predicate first: head comparison, then one linear pass over the
/// loop's own top-level children (never its operands' subtrees) to place the
/// clause keywords. The whole-form occurrence count runs last, only for a loop
/// whose entire clause shape has already matched.
pub fn examine(
    view: &ExpressionView,
    collect_into_form_count: &mut usize,
    violations: &mut Vec<LoopCollectIntoImmediatelyReturnedItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["loop"]) {
        return;
    }
    let Some(shape) = read_collect_into_shape(view) else {
        return;
    };
    *collect_into_form_count += 1;

    // The accumulator may appear only where the shape put it: `into acc` and
    // `(return acc)`. A third occurrence — a guard, a `do (print acc)`, a
    // second accumulation — makes the variable load-bearing.
    if count_symbol_occurrences(view, &shape.accumulator) != 2 {
        return;
    }
    violations.push(LoopCollectIntoImmediatelyReturnedItem {
        span: view.span,
        accumulator: shape.accumulator,
        verb: shape.verb,
    });
}

/// Collects every `loop` in one file that collects into an accumulator only to
/// return it, with the number of loops matching the clause shape as the
/// denominator beside them.
pub fn build_loop_collect_into_immediately_returned_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LoopCollectIntoImmediatelyReturnedItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("collect_into_form_count", json!(0))],
        ));
    }

    let mut collect_into_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut collect_into_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("collect_into_form_count", json!(collect_into_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<LoopCollectIntoImmediatelyReturnedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_loop_collect_into_immediately_returned_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_the_canonical_shape() {
        let violations =
            report("(loop for x in items collect (f x) into acc finally (return acc))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].accumulator, "acc");
        assert_eq!(violations[0].verb, "collect");
    }

    #[test]
    fn flags_the_collecting_spelling() {
        let violations =
            report("(loop for x in items collecting x into acc finally (return acc))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].verb, "collecting");
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(
            report("(CL:LOOP FOR x IN items COLLECT x INTO Acc FINALLY (CL:RETURN acc))")
                .findings
                .len(),
            1
        );
    }

    // -- the `named` trap ----------------------------------------------------

    /// SBCL: `(block nil (list :v (loop named outer … finally (return a))))`
    /// evaluates to `(1 2)`, not `(:V (1 2))` — the `return` leaves the *block*,
    /// not the loop, so the rewrite would change what the caller sees.
    #[test]
    fn does_not_flag_a_named_loop() {
        assert!(
            report("(loop named outer for x in items collect x into acc finally (return acc))")
                .findings
                .is_empty()
        );
    }

    // -- the accumulator-is-load-bearing guards -------------------------------

    #[test]
    fn does_not_flag_when_the_accumulator_is_read_in_the_body() {
        assert!(
            report(
                "(loop for x in items collect x into acc \
                 when (> (length acc) 3) do (report acc) finally (return acc))"
            )
            .findings
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_two_accumulations() {
        assert!(
            report(
                "(loop for x in items collect x into acc sum x into total finally (return acc))"
            )
            .findings
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_second_into_naming_the_same_variable() {
        assert!(
            report(
                "(loop for x in items collect x into acc collect (g x) into acc \
                 finally (return acc))"
            )
            .findings
            .is_empty()
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_plain_collect() {
        assert!(
            report("(loop for x in items collect (f x))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_finally_returning_something_else() {
        assert!(
            report("(loop for x in items collect x into acc finally (return (length acc)))")
                .findings
                .is_empty()
        );
        assert!(
            report("(loop for x in items collect x into acc finally (return other))")
                .findings
                .is_empty()
        );
    }

    /// A `finally` with a second form is `loop-unreachable-finally-clause`'s
    /// subject, not this rule's; requiring the epilogue to be exactly the one
    /// `return` keeps the two disjoint.
    #[test]
    fn does_not_flag_a_finally_with_more_than_the_return() {
        assert!(
            report("(loop for x in items collect x into acc finally (print acc) (return acc))")
                .findings
                .is_empty()
        );
        assert!(
            report("(loop for x in items collect x into acc finally (return acc) (print :done))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_return_from() {
        assert!(
            report("(loop for x in items collect x into acc finally (return-from f acc))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_non_collect_verb() {
        assert!(
            report("(loop for x in items sum x into total finally (return total))")
                .findings
                .is_empty()
        );
        assert!(
            report("(loop for x in items append x into acc finally (return acc))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_with_no_finally() {
        assert!(
            report("(loop for x in items collect x into acc)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_two_finally_clauses() {
        assert!(
            report(
                "(loop for x in items collect x into acc finally (print :a) finally (return acc))"
            )
            .findings
            .is_empty()
        );
    }

    /// A `collect` whose *operand* happens to be the symbol `into` is not the
    /// `into` clause keyword. Positions are fixed by the grammar.
    #[test]
    fn does_not_flag_a_misplaced_into() {
        assert!(
            report("(loop for x in items collect into finally (return into))")
                .findings
                .is_empty()
        );
    }

    // -- the five quote shapes ------------------------------------------------

    const CANONICAL: &str = "(loop for x in items collect x into acc finally (return acc))";

    #[test]
    fn plain_code_fires() {
        assert!(fires(CANONICAL));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires(&format!("'{CANONICAL}")));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires(&format!("(quote {CANONICAL})")));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires(&format!("'(y ,{CANONICAL})")));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert!(fires(&format!("`(y ,{CANONICAL})")));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires(&format!("(defmacro m () `{CANONICAL})")));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        let source = format!("(format t \"{}\")", CANONICAL.replace('"', ""));
        assert!(report(&source).findings.is_empty());
        assert!(!fires(&source));
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CANONICAL, Dialect::EmacsLisp).expect("parse");
        let report = build_loop_collect_into_immediately_returned_report(
            Path::new("app.el"),
            Dialect::EmacsLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    /// The denominator counts loops whose *clause shape* matched, which is the
    /// population the occurrence guard then filters — a usable coverage number
    /// rather than "every loop in the file".
    #[test]
    fn the_summary_counts_every_matching_clause_shape() {
        let report = report(
            "(loop for x in items collect x into acc finally (return acc))\n\
             (loop for x in items collect x into b do (print b) finally (return b))\n\
             (loop for x in items collect x)\n",
        );
        assert_eq!(report.summary, vec![("collect_into_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_accumulator() {
        let report = report(&format!("(defun f (items)\n  {CANONICAL})\n"));
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "loop-collect-into-immediately-returned");
        assert_eq!(finding.text_columns(), vec!["acc".to_owned()]);
    }
}
