use super::*;

#[test]
fn introduces_single_selected_occurrence_by_default() {
    assert_plan(
        "(defun render () (+ (* width height) margin (* width height)))",
        "0.3.1",
        false,
        1,
        0,
        "(defun render () (let ((product (* width height))) (+ product margin (* width height))))",
    );
}

#[test]
fn introduce_let_preserves_dialect_reader_collisions() {
    let cases = [(
        Dialect::Janet,
        "(+ (* width height) margin)\n# ignored ))",
        "0.1",
        "(let [product (* width height)] (+ product margin))\n# ignored ))",
    )];

    for (dialect, input, path, expected) in cases {
        assert_plan_with_dialect(input, dialect, path, false, 1, 0, expected);
    }
}

#[test]
fn introduces_dialect_appropriate_let_for_every_verified_dialect() {
    let cases = [
        (
            Dialect::CommonLisp,
            "(let ((product (* width height))) (list product outer))",
        ),
        (
            Dialect::EmacsLisp,
            "(let ((product (* width height))) (list product outer))",
        ),
        (
            Dialect::Scheme,
            "(let ((product (* width height))) (list product outer))",
        ),
        (
            Dialect::Clojure,
            "(let [product (* width height)] (list product outer))",
        ),
        (
            Dialect::Janet,
            "(let [product (* width height)] (list product outer))",
        ),
        (
            Dialect::Fennel,
            "(let [product (* width height)] (list product outer))",
        ),
    ];

    for (dialect, expected) in cases {
        assert_plan_with_dialect(
            "(list (* width height) outer)",
            dialect,
            "0.1",
            false,
            1,
            0,
            expected,
        );
    }
}

#[test]
fn introduces_all_structurally_equivalent_occurrences() {
    assert_plan(
        "(defun render () (+ (* width height) margin (*  width height)))",
        "0.3.1",
        true,
        2,
        0,
        "(defun render () (let ((product (* width height))) (+ product margin product)))",
    );
}

#[test]
fn keeps_different_atom_values_out_of_all_occurrences() {
    assert_plan(
        "(defun render () (+ (* width height) (* width depth)))",
        "0.3.1",
        true,
        1,
        0,
        "(defun render () (let ((product (* width height))) (+ product (* width depth))))",
    );
}
