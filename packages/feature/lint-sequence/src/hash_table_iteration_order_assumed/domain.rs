//! Common Lisp hash-table-order detection: an element picked *by position* out
//! of a hash table's iteration.
//!
//! CLHS 18.1 leaves the order in which `maphash` and `loop … being the
//! hash-keys` visit a table's entries unspecified, and `with-hash-table-iterator`
//! likewise. `(first (loop for k being the hash-keys of table collect k))` is
//! therefore "some key", not "the first key": it may differ between two
//! implementations, between two versions of one implementation, and between two
//! tables with the same contents but different insertion or rehash history.
//! Code that reads one element by position out of such a list is relying on
//! something the standard does not promise.
//!
//! # What is reported
//!
//! An order-sensitive accessor — `car`, `first`, `second`, `third`, `last`,
//! `nth`, `elt` — applied *directly* to a form that produces a list from a hash
//! table's iteration:
//!
//! - a `loop` whose own clauses say `being … hash-key(s)`/`hash-value(s)` and
//!   which accumulates with `collect`/`append`/`nconc`, or
//! - a call to `hash-table-keys`/`hash-table-values`/`hash-table-alist`/
//!   `hash-table-plist`, whose results come out in the table's iteration order
//!   too.
//!
//! # What is *not* reported, and why
//!
//! - **A sorted result.** `(first (sort (hash-table-keys table) #'string<))` has
//!   `sort` as the accessor's argument, not a producer, so it cannot match; and
//!   a `loop` that sorts inside itself, or that reshapes its accumulation with
//!   `into`/`finally`, is skipped explicitly. Sorting is the fix this rule is
//!   asking for, so firing on it would be firing on the remedy.
//! - **Iterating for effect.** A `maphash` that pushes onto a list which some
//!   later form reads positionally is the same defect, and is deliberately out
//!   of scope: connecting the two needs dataflow between separate forms, and
//!   guessing it wrong means reporting correct code. This rule prefers the
//!   false negative.
//! - **`(length …)`, `(remove … )`, membership, or any other order-blind use.**
//!   Only the positional accessors are heads here, so an order-blind use of the
//!   very same list is never a finding.
//! - **A nested `loop`'s clauses.** Only the accessor's own argument's *direct*
//!   children are read as clauses, so a `loop` that merely contains another
//!   `loop` doing the hash iteration is not itself one.
//!
//! Report-only: the repair is to sort the keys, or to stop reading one by
//! position, and which of those is right is a decision about the program.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, for_each_subview, is_paren_list, list_head, symbol_in, unqualified,
};
use serde_json::{Value, json};

use crate::support::for_each_evaluated_subview;

/// The order-sensitive accessors, each with the argument position its sequence
/// occupies and the arities it accepts.
///
/// `(nth n list)` puts its sequence second; `(elt sequence n)` puts it first;
/// `(last list &optional n)` accepts one or two operands.
/// One accessor: its name, the argument position its sequence occupies, and the
/// smallest and largest operand counts it accepts.
type Accessor = (&'static str, usize, usize, usize);

/// The accessor a head names, or `None`.
///
/// # Cost
///
/// Switching on the first byte before comparing the name is what keeps this off
/// the batch's hot path. The obvious spelling — a seven-entry table and
/// `iter().find(|(name, ..)| symbol_in(head, &[name]))` — re-strips the package
/// qualifier and case-folds the whole string once *per entry*, up to seven
/// times for `elt`, and measurably so: at 40 000 invocations over a
/// zero-finding file it cost 1609 µs against a control rule's 226 µs over
/// 12 000, or 40 ns per invocation against 19 ns. The first byte settles which
/// of the seven it could be in one comparison, leaving exactly one string
/// comparison to confirm it. Nothing here is a heuristic: the seven names begin
/// with seven different letters.
fn accessor_of(head: &str) -> Option<Accessor> {
    let name = unqualified(head);
    let candidate: Accessor = match name.as_bytes().first()?.to_ascii_lowercase() {
        b'c' => ("car", 1, 2, 2),
        b'f' => ("first", 1, 2, 2),
        b's' => ("second", 1, 2, 2),
        b't' => ("third", 1, 2, 2),
        b'l' => ("last", 1, 2, 3),
        b'n' => ("nth", 2, 3, 3),
        b'e' => ("elt", 1, 3, 3),
        _ => return None,
    };
    name.eq_ignore_ascii_case(candidate.0).then_some(candidate)
}

/// Library functions that hand back a hash table's contents as a fresh list, in
/// the table's own unspecified iteration order. All four are alexandria's.
const PRODUCERS: [&str; 4] = [
    "hash-table-keys",
    "hash-table-values",
    "hash-table-alist",
    "hash-table-plist",
];

/// The `loop` clause words that name a hash-table iteration path.
const HASH_ITERATION_WORDS: [&str; 4] = ["hash-key", "hash-keys", "hash-value", "hash-values"];

/// The `loop` accumulation clauses that build a list.
const ACCUMULATIONS: [&str; 6] = [
    "collect",
    "collecting",
    "append",
    "appending",
    "nconc",
    "nconcing",
];

/// The `loop` clause words that hand the accumulated list somewhere this rule
/// cannot follow — most importantly to a `finally` that may sort it.
const RESHAPING_WORDS: [&str; 2] = ["into", "finally"];

/// Operators that impose an order on their argument, making a positional read
/// of the result well-defined.
const ORDERING_OPERATORS: [&str; 3] = ["sort", "stable-sort", "merge"];

#[derive(Debug, Clone)]
pub struct HashOrderItem {
    /// The span of the whole `(first (loop …))` form.
    pub span: ByteSpan,
    /// The accessor's normalized name.
    pub accessor: String,
    /// The span of the producing form.
    pub producer_span: ByteSpan,
}

impl Finding for HashOrderItem {
    fn kind(&self) -> &'static str {
        "hash-order-assumed"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.accessor.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("accessor", json!(self.accessor)),
            ("producer_span", span_json(self.producer_span)),
        ]
    }

    fn message(&self) -> String {
        message_for(&self.accessor)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(accessor: &str) -> String {
    format!(
        "{accessor} reads one element by position out of a hash table's iteration, \
         whose order the standard leaves unspecified; sort the result first, or \
         stop depending on which element comes out"
    )
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Whether `view` is a `loop` whose own clauses iterate a hash table and
/// accumulate a list, without handing that list anywhere this rule cannot see.
///
/// Only the loop's *direct* children are read, and only the atoms among them: a
/// clause word is an atom of the `loop` form itself, so a nested `loop` doing
/// the iteration does not make its parent one.
fn is_hash_iteration_loop(view: &ExpressionView) -> bool {
    let mut iterates = false;
    let mut accumulates = false;
    for child in &view.children {
        let Some(text) = atom_text(child) else {
            continue;
        };
        if symbol_in(text, &RESHAPING_WORDS) {
            return false;
        }
        iterates = iterates || symbol_in(text, &HASH_ITERATION_WORDS);
        accumulates = accumulates || symbol_in(text, &ACCUMULATIONS);
    }
    iterates && accumulates
}

/// Whether any form anywhere inside `view` imposes an order on the result.
///
/// Walked only for a form that has already been established to be a
/// hash-iterating, list-accumulating `loop` under a positional accessor, which
/// is rare enough that the walk's cost never reaches ordinary code.
fn imposes_an_order(view: &ExpressionView) -> bool {
    let mut ordered = false;
    for_each_subview(view, |subview| {
        ordered =
            ordered || list_head(subview).is_some_and(|head| symbol_in(head, &ORDERING_OPERATORS));
    });
    ordered
}

/// Whether `view` produces a list in a hash table's own iteration order.
fn is_unordered_producer(view: &ExpressionView) -> bool {
    if !is_paren_list(view) {
        return false;
    }
    let Some(head) = list_head(view) else {
        return false;
    };
    if symbol_in(head, &PRODUCERS) {
        return true;
    }
    symbol_in(head, &["loop"]) && is_hash_iteration_loop(view) && !imposes_an_order(view)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// # Cost
///
/// `car`, `first` and `nth` are dense in ordinary code, so the predicates are
/// ordered cheapest-first: `accessor_of` (one byte switch and one string
/// comparison), then the arity, then `is_paren_list` on the sequence operand —
/// which rejects `(car xs)`, `(first row)` and `(nth i items)`, the shapes that
/// actually occur, before anything reads a clause or compares a producer name.
/// Only an accessor whose operand is a *call* compares four producer names, and
/// only a `loop` operand scans clauses.
pub fn examine(
    view: &ExpressionView,
    accessor_form_count: &mut usize,
    violations: &mut Vec<HashOrderItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((name, position, min_arity, max_arity)) = accessor_of(head) else {
        return;
    };
    *accessor_form_count += 1;

    if view.children.len() < min_arity || view.children.len() > max_arity {
        return;
    }
    let Some(sequence) = view.children.get(position) else {
        return;
    };
    if !is_unordered_producer(sequence) {
        return;
    }

    violations.push(HashOrderItem {
        span: view.span,
        accessor: name.to_owned(),
        producer_span: sequence.span,
    });
}

/// Collects every positional read of a hash table's iteration in one file, with
/// the number of accessor forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no order assumptions here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn collect_hash_order_assumptions(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<HashOrderItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("accessor_form_count", json!(0))],
        ));
    }

    let mut accessor_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut accessor_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("accessor_form_count", json!(accessor_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<HashOrderItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_hash_order_assumptions(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect hash order assumptions")
    }

    /// `examine` applied to every node of a source, which is what the lint rule
    /// sees through the dispatcher — quoting and all.
    fn examined(input: &str) -> Vec<HashOrderItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        let mut count = 0;
        let mut violations = Vec::new();
        for_each_subview(&tree.root_view(), |view| {
            examine(view, &mut count, &mut violations);
        });
        violations
    }

    fn accessors(input: &str) -> Vec<String> {
        examined(input)
            .into_iter()
            .map(|item| item.accessor)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_positional_read_of_a_hash_iterating_loop() {
        assert_eq!(
            accessors("(first (loop for k being the hash-keys of table collect k))"),
            vec!["first"]
        );
    }

    #[test]
    fn flags_every_accessor_at_its_own_operand_position() {
        assert_eq!(accessors("(car (hash-table-keys table))"), vec!["car"]);
        assert_eq!(
            accessors("(second (hash-table-keys table))"),
            vec!["second"]
        );
        assert_eq!(
            accessors("(third (hash-table-values table))"),
            vec!["third"]
        );
        assert_eq!(accessors("(last (hash-table-keys table))"), vec!["last"]);
        assert_eq!(accessors("(last (hash-table-keys table) 2)"), vec!["last"]);
        // `nth` puts its sequence second and `elt` puts it first.
        assert_eq!(accessors("(nth 0 (hash-table-keys table))"), vec!["nth"]);
        assert_eq!(accessors("(elt (hash-table-keys table) 0)"), vec!["elt"]);
    }

    #[test]
    fn flags_the_hash_value_and_singular_clause_spellings() {
        assert_eq!(
            accessors("(first (loop for v being the hash-values of table collect v))"),
            vec!["first"]
        );
        assert_eq!(
            accessors("(first (loop for k being each hash-key of table collect k))"),
            vec!["first"]
        );
    }

    #[test]
    fn flags_the_append_and_nconc_accumulations() {
        assert_eq!(
            accessors("(first (loop for v being the hash-values of table append v))"),
            vec!["first"]
        );
        assert_eq!(
            accessors("(first (loop for v being the hash-values of table nconc v))"),
            vec!["first"]
        );
    }

    #[test]
    fn flags_the_alist_and_plist_producers() {
        assert_eq!(accessors("(first (hash-table-alist table))"), vec!["first"]);
        assert_eq!(accessors("(first (hash-table-plist table))"), vec!["first"]);
    }

    #[test]
    fn flags_uppercase_and_package_qualified_heads() {
        assert_eq!(accessors("(FIRST (HASH-TABLE-KEYS table))"), vec!["first"]);
        assert_eq!(
            accessors("(cl:first (alexandria:hash-table-keys table))"),
            vec!["first"]
        );
    }

    // -- the trap: a sorted result is the remedy, not the defect --------------

    #[test]
    fn does_not_flag_a_sorted_producer() {
        assert!(accessors("(first (sort (hash-table-keys table) #'string<))").is_empty());
        assert!(accessors("(first (stable-sort (hash-table-keys table) #'<))").is_empty());
    }

    #[test]
    fn does_not_flag_a_loop_that_sorts_inside_itself() {
        assert!(
            accessors(
                "(first (loop for k being the hash-keys of table \
                 collect (sort (copy-list k) #'<)))"
            )
            .is_empty()
        );
    }

    /// The discriminating test for the `into`/`finally` guard. The version
    /// below that spells `sort` proves nothing about it: `imposes_an_order`
    /// suppresses that one first, so the reshaping guard could be deleted and
    /// no test would notice. Here nothing names an ordering operator, and what
    /// suppresses it is only that `finally` can do anything to the accumulated
    /// list — including ordering it by means this rule cannot read.
    #[test]
    fn does_not_flag_a_loop_that_reshapes_its_accumulation_without_naming_sort() {
        assert!(
            accessors(
                "(first (loop for k being the hash-keys of table \
                 collect k into keys finally (return (arrange keys))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_that_hands_its_accumulation_to_finally() {
        assert!(
            accessors(
                "(first (loop for k being the hash-keys of table \
                 collect k into keys finally (return (sort keys #'string<))))"
            )
            .is_empty()
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_an_order_blind_use_of_the_same_list() {
        assert!(accessors("(length (hash-table-keys table))").is_empty());
        assert!(accessors("(remove nil (hash-table-keys table))").is_empty());
        assert!(accessors("(member x (hash-table-keys table))").is_empty());
    }

    #[test]
    fn does_not_flag_an_accessor_on_an_ordinary_list() {
        assert!(accessors("(first xs)").is_empty());
        assert!(accessors("(nth 0 (compute-rows table))").is_empty());
        assert!(accessors("(car (sort xs #'<))").is_empty());
    }

    #[test]
    fn does_not_flag_a_loop_that_does_not_iterate_a_hash_table() {
        assert!(accessors("(first (loop for x in xs collect x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_hash_iterating_loop_that_accumulates_nothing() {
        // Nothing positional can be read out of a scalar.
        assert!(accessors("(first (loop for k being the hash-keys of table count k))").is_empty());
    }

    /// Clause words are atoms of the `loop` form itself, so a `loop` that
    /// merely contains another one is not a hash iteration.
    #[test]
    fn does_not_flag_an_outer_loop_whose_inner_loop_iterates_the_hash_table() {
        assert!(
            accessors(
                "(first (loop for table in tables \
                 collect (loop for k being the hash-keys of table collect k)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_wrong_arity_accessor() {
        assert!(accessors("(first)").is_empty());
        assert!(accessors("(first (hash-table-keys table) extra)").is_empty());
        assert!(accessors("(nth (hash-table-keys table))").is_empty());
    }

    /// `maphash` results correlated with a later positional read are out of
    /// scope on purpose.
    #[test]
    fn does_not_flag_a_maphash_that_pushes_onto_a_list() {
        assert!(
            accessors(
                "(let ((acc '())) (maphash (lambda (k v) (declare (ignore v)) (push k acc)) table) \
                 (first acc))"
            )
            .is_empty()
        );
    }

    // -- quoting and strings, through the report path ------------------------

    #[test]
    fn the_report_skips_the_five_quote_shapes() {
        for source in [
            "'(first (hash-table-keys table))",
            "(quote (first (hash-table-keys table)))",
            "`(first (hash-table-keys table))",
            "'(a ,(first (hash-table-keys table)))",
            "'(outer (first (hash-table-keys table)))",
        ] {
            assert!(
                report(source).findings.is_empty(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            report("`(a ,(first (hash-table-keys table)))")
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn a_form_inside_a_string_literal_is_not_a_form() {
        assert!(
            report("(format nil \"(first (hash-table-keys table))\")")
                .findings
                .is_empty()
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(first (hash-table-keys t))", Dialect::Clojure)
            .expect("parse");
        let report = collect_hash_order_assumptions(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_accessor_scanned_not_only_the_flagged_ones() {
        let report = report("(first (hash-table-keys table))\n(first xs)\n(car ys)\n");
        assert_eq!(report.summary, vec![("accessor_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_producer_span() {
        let report = report("(defun f (table)\n  (first (hash-table-keys table)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "hash-order-assumed");
        assert_eq!(finding.text_columns(), vec!["first".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("accessor", json!("first")),
                ("producer_span", span_json(finding.producer_span)),
            ]
        );
        assert!(finding.message().contains("unspecified"));
    }
}
