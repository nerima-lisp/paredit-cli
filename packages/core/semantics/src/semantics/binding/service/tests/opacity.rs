//! What costs a scope its transparency, and what no longer does.
//!
//! Each case here stands for one claim in the policy tables: a form is
//! transparent because it cannot reassign a binding, or opaque because nothing
//! in the source rules out that it does. The pairing matters as much as either
//! half — a table entry that made everything transparent would pass the
//! positive tests and be worthless.

use super::{binding_at, build};
use crate::semantics::binding::OpacityCauseKind;

/// Whether the binding defined at `offset` sees a fully readable scope.
fn is_transparent(input: &str, offset: usize) -> bool {
    let table = build(input);
    table
        .binding(binding_at(&table, offset))
        .opacity()
        .is_transparent()
}

/// What first cost the binding defined at `offset` its transparency.
fn cause_kind(input: &str, offset: usize) -> Option<OpacityCauseKind> {
    let table = build(input);
    table
        .binding(binding_at(&table, offset))
        .opacity_cause()
        .map(|cause| cause.kind())
}

#[test]
fn a_declaration_specifier_does_not_cost_a_scope_its_transparency() {
    // `declare` is registered, so the walk descends into it and reads
    // `(ignore y)` as a call. Before the declaration table that unknown head
    // made the whole `let` opaque — for a form CLHS gives no run-time
    // semantics at all.
    //
    // The docstring is what puts the `declare` where it is felt: `body` skips
    // only *leading* declarations, and a docstring ends that run.
    let input = r#"(let ((x 1) (y 2)) "doc" (declare (ignore y)) x)"#;
    assert!(is_transparent(input, 7));
}

#[test]
fn a_nested_optimize_quality_does_not_cost_a_scope_its_transparency() {
    // `(optimize (speed 3))` nests: clearing `optimize` alone would leave the
    // walk tripping over `speed` and the scope exactly as opaque as before.
    let input = r#"(let ((x 1)) "doc" (declare (optimize (speed 3) (safety 0))) x)"#;
    assert!(is_transparent(input, 7));
}

#[test]
fn ignore_errors_does_not_cost_a_scope_its_transparency() {
    // `(ignore-errors body)` is `(handler-case (progn body) (error (c) …))`:
    // the body is evaluated where it is written, so any assignment in it is
    // already visible to the assignment collector.
    //
    // The body is `(length x)` rather than a call to something invented: an
    // unknown head inside would make the scope opaque on its own and the test
    // would pass for the wrong reason.
    let input = "(let ((x 1)) (ignore-errors (length x)) x)";
    assert!(is_transparent(input, 7));
}

#[test]
fn an_unknown_head_still_costs_a_scope_its_transparency() {
    // The rule the tables exist to preserve: `my-macro` could expand into
    // `(setq x …)` with nothing in the source to show for it.
    let input = "(let ((x 1)) (my-macro x) x)";
    assert!(!is_transparent(input, 7));
    assert_eq!(cause_kind(input, 7), Some(OpacityCauseKind::UnknownHead));
}

#[test]
fn the_cause_points_at_the_head_so_a_caller_can_name_it() {
    let input = "(let ((x 1)) (my-macro x) x)";
    let table = build(input);
    let cause = table
        .binding(binding_at(&table, 7))
        .opacity_cause()
        .expect("an opaque scope records why");
    assert_eq!(cause.site().slice(input), "my-macro");
}

#[test]
fn a_computed_head_is_told_apart_from_an_unknown_name() {
    // `((lambda …) x)` has no name to register, so no table entry could ever
    // make it transparent. Counting it among the unknown *heads* would put a
    // row in the ranking that no work can ever remove.
    let input = "(let ((x 1)) ((lambda (y) y) x) x)";
    assert_eq!(cause_kind(input, 7), Some(OpacityCauseKind::UnreadableHead));
}

#[test]
fn quoted_data_is_inert_and_leaves_the_scope_readable() {
    // `'(setq x 2)` is a three-element list. Nothing in it is evaluated, so
    // there is no assignment for the scope to fear — even though the walk
    // stops there, because quoted data holds no live references either.
    for input in [
        "(let ((x 1)) '(setq x 2) x)",
        "(let ((x 1)) (quote (setq x 2)) x)",
    ] {
        assert!(is_transparent(input, 7), "{input}");
    }
}

#[test]
fn read_time_evaluation_still_costs_a_scope_its_transparency() {
    // `#.` is the one prefix whose result re-enters the program as *code*:
    // the reader could have computed `(setq x 2)` and spliced it into this
    // body with nothing in the source to show for it.
    let input = "(let ((x 1)) #.(compute) x)";
    assert_eq!(
        cause_kind(input, 7),
        Some(OpacityCauseKind::QuotedOrReadTime)
    );
}

#[test]
fn a_reader_conditional_still_costs_a_scope_its_transparency() {
    // `#+sbcl` decides at read time whether the text after it exists at all.
    let input = "(let ((x 1)) #+sbcl (setq x 2) x)";
    assert_eq!(cause_kind(input, 7), Some(OpacityCauseKind::ReaderDispatch));
}

#[test]
fn a_transparent_scope_records_no_cause() {
    assert_eq!(cause_kind("(let ((x 1)) (+ x 1))", 7), None);
}
