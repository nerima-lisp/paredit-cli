use super::*;

#[test]
fn skips_occurrences_inside_locally_special_declaration_scope() {
    assert_plan(
        "(progn (compute) (locally (declare (special product)) (compute)))",
        "0.1",
        true,
        1,
        1,
        "(let ((product (compute))) (progn product (locally (declare (special product)) (compute))))",
    );
}

#[test]
fn rejects_selection_inside_locally_special_declaration_scope() {
    assert_shadowed_error(
        "(progn (compute) (locally (declare (special product)) (compute)))",
        "0.2.2",
    );
}

#[test]
fn leaves_locally_body_lexical_when_special_name_differs() {
    assert_plan(
        "(progn (compute) (locally (declare (special other)) (compute)))",
        "0.1",
        true,
        2,
        0,
        "(let ((product (compute))) (progn product (locally (declare (special other)) product)))",
    );
}

#[test]
fn rejects_selection_inside_let_special_declaration_scope() {
    assert_shadowed_error(
        "(defun render () (let () (declare (special product)) (* width height)))",
        "0.3.3",
    );
}

#[test]
fn skips_occurrences_inside_defun_special_declaration_scope() {
    assert_plan(
        "(progn (compute) (defun render () (declare (special product)) (compute)))",
        "0.1",
        true,
        1,
        1,
        "(let ((product (compute))) (progn product (defun render () (declare (special product)) (compute))))",
    );
}

#[test]
fn leaves_let_body_lexical_when_special_name_differs() {
    assert_plan(
        "(progn (compute) (let () (declare (special other)) (compute)))",
        "0.1",
        true,
        2,
        0,
        "(let ((product (compute))) (progn product (let () (declare (special other)) product)))",
    );
}
