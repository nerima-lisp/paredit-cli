//! Which forms count as reassigning a binding.

use super::{assignment_labels, binding_at, build};
use crate::domain::dialect::Dialect;
use crate::domain::semantics::binding::policy::{PlacePositions, assignment_forms};

/// A call to `head` that puts `x` and `y` in its place positions.
///
/// The place text is a parameter rather than something a caller patches in
/// afterwards, because the shapes differ in where the places nest: `assert`
/// holds its places inside a sublist, where a textual substitution on the
/// finished call would miss them.
fn assignment_call(head: &str, places: PlacePositions, x: &str, y: &str) -> String {
    let arguments = match places {
        // `(setq x 1 y 2)`
        PlacePositions::Pairs => format!("{x} 1 {y} 2"),
        PlacePositions::FirstArgument => x.to_owned(),
        PlacePositions::SecondArgument => format!("1 {x}"),
        PlacePositions::EveryArgument => format!("{x} {y}"),
        PlacePositions::EveryArgumentButLast => format!("{x} {y} 1"),
        // `(assert (> x 0) (x y))` / `(multiple-value-setq (x y) form)`
        PlacePositions::NestedInArgument(1) => format!("({x} {y}) nil"),
        PlacePositions::NestedInArgument(_) => format!("t ({x} {y})"),
    };
    format!("({head} {arguments})")
}

#[test]
fn every_assignment_head_records_its_bare_variable_places() {
    for form in assignment_forms(Dialect::CommonLisp) {
        let call = assignment_call(form.head(), form.places(), "x", "y");
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
        let call = assignment_call(form.head(), form.places(), "(car x)", "(car y)");
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
fn check_type_records_the_write_its_store_value_restart_can_perform() {
    // `(check-type x integer)` reads as a pure assertion and is not one: the
    // `store-value` restart writes a new value into `x`. Registering the head
    // as transparent without this would let the value layer propagate `1`
    // into a reference the program may have replaced.
    let input = "(let ((x 1)) (check-type x integer) x)";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert_eq!(assignment_labels(&table, x, input), vec!["x@25"]);
    assert!(!table.binding(x).is_propagatable());
}

#[test]
fn assert_records_the_write_its_restart_can_perform_on_each_place_it_names() {
    // `(assert (> x 0) (x))` offers a restart that writes `x`, and says
    // nothing about `y`.
    let input = "(let ((x 1) (y 2)) (assert (> x 0) (x)) (list x y))";

    let table = build(input);
    assert_eq!(
        assignment_labels(&table, binding_at(&table, 7), input),
        vec!["x@36"]
    );
    assert!(
        table
            .binding(binding_at(&table, 13))
            .assignments()
            .is_empty()
    );
}

#[test]
fn an_assert_with_no_place_list_leaves_the_scope_readable() {
    // The shape that actually pays: a bare `(assert test)` writes nothing, and
    // registering the head is what lets the binding keep its value.
    let input = "(let ((x 1)) (assert (> x 0)) x)";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert!(table.binding(x).assignments().is_empty());
    assert!(table.binding(x).is_propagatable());
}

#[test]
fn a_place_list_still_costs_the_scope_its_transparency() {
    // A known limit, pinned so it is not mistaken for a win. `(x)` in the
    // place-list position is walked as an ordinary call, and `x` is not a
    // registered head, so the scope goes opaque there — conservatively, and
    // for a reason that has nothing to do with `assert` itself.
    //
    // The assignment entry above is what keeps the answer *right* rather than
    // merely conservative: it survives if that descent is ever refined.
    let input = "(let ((x 1) (y 2)) (assert (> x 0) (x)) (list x y))";

    let table = build(input);
    assert!(
        !table
            .binding(binding_at(&table, 13))
            .opacity()
            .is_transparent()
    );
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
