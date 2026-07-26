//! Which forms count as reassigning a binding.

use super::{assignment_labels, binding_at, build};
use crate::domain::dialect::Dialect;
use crate::domain::semantics::binding::policy::{PlacePositions, assignment_forms};

/// A call to `head` whose places are bare variables, and the offsets of those
/// places, for a `(let ((x 1) (y 2)) ...)` wrapper.
fn assignment_call(head: &str, places: PlacePositions) -> String {
    let arguments: Vec<&str> = match places {
        // `(setq x 1 y 2)`
        PlacePositions::Pairs => vec!["x", "1", "y", "2"],
        PlacePositions::FirstArgument => vec!["x"],
        PlacePositions::SecondArgument => vec!["1", "x"],
        PlacePositions::EveryArgument => vec!["x", "y"],
        PlacePositions::EveryArgumentButLast => vec!["x", "y", "1"],
    };
    format!("({head} {})", arguments.join(" "))
}

#[test]
fn every_assignment_head_records_its_bare_variable_places() {
    for form in assignment_forms(Dialect::CommonLisp) {
        let call = assignment_call(form.head(), form.places());
        let input = format!("(let ((x 1) (y 2)) {call})");

        let table = build(&input);
        let x = binding_at(&table, 7);
        assert!(
            !table.binding(x).assignments().is_empty(),
            "{} must record an assignment to x in {input}",
            form.head()
        );
        assert!(
            !table.binding(x).is_propagatable(),
            "{} must stop propagation in {input}",
            form.head()
        );
    }
}

#[test]
fn every_assignment_head_ignores_a_list_place() {
    // `(setf (car x) 1)` mutates what `x` points at. The binding still holds
    // the value it was given, so recording it as reassigned would block a
    // propagation that is perfectly sound.
    for form in assignment_forms(Dialect::CommonLisp) {
        let call = assignment_call(form.head(), form.places())
            .replace(" x", " (car x)")
            .replace(" y", " (car y)");
        let input = format!("(let ((x 1) (y 2)) {call})");

        let table = build(&input);
        assert!(
            table
                .binding(binding_at(&table, 7))
                .assignments()
                .is_empty(),
            "{} must not record a list place in {input}",
            form.head()
        );
    }
}

#[test]
fn a_list_place_still_reads_the_variable_inside_it() {
    let input = "(let ((x 1)) (setf (car x) 2))";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert!(table.binding(x).assignments().is_empty());
    assert_eq!(
        table.binding(x).references().len(),
        1,
        "`x` inside the place is still read"
    );
}

#[test]
fn an_assignment_lands_on_the_innermost_binding_of_the_name() {
    let input = "(let ((x 1)) (let ((x 2)) (setq x 3)))";

    let table = build(input);
    let outer = binding_at(&table, 7);
    let inner = binding_at(&table, 20);
    assert!(
        table.binding(outer).assignments().is_empty(),
        "the outer `x` is shadowed and must not be credited with the setq"
    );
    assert_eq!(assignment_labels(&table, inner, input), vec!["x@32"]);
}

#[test]
fn only_the_place_positions_of_a_head_are_assignments() {
    // `(push x place)`: `x` is read, `place` is written. Reading the first
    // argument as a place would invent a reassignment.
    let input = "(let ((x 1) (place nil)) (push x place))";

    let table = build(input);
    assert!(
        table
            .binding(binding_at(&table, 7))
            .assignments()
            .is_empty()
    );
    assert_eq!(
        assignment_labels(&table, binding_at(&table, 13), input),
        vec!["place@33"]
    );
}

#[test]
fn an_assignment_to_a_free_name_is_recorded_nowhere() {
    let input = "(let ((x 1)) (setq y 2))";

    let table = build(input);
    assert!(
        table
            .binding(binding_at(&table, 7))
            .assignments()
            .is_empty()
    );
    assert_eq!(table.bindings().len(), 1);
}

#[test]
fn a_dialect_spelling_from_another_dialect_is_not_an_assignment() {
    // `set!` belongs to Scheme; in a Common Lisp file it is an ordinary call.
    let input = "(let ((x 1)) (set! x 2))";

    let table = build(input);
    assert!(
        table
            .binding(binding_at(&table, 7))
            .assignments()
            .is_empty()
    );
}
