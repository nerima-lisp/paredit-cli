use super::*;

#[test]
fn skips_nested_labels_calls_when_renaming_outer_flet() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) x)) (labels ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(flet ((bar (x) x))",
            "(labels ((foo (y) (foo y))) (foo 1))",
            "(bar 2)"
        ]
    );
}

#[test]
fn renames_flet_calls_inside_bare_lambda_bodies_without_touching_shadowing_parameter() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) x)) (let ((fn (lambda (foo) (foo 1)))) (funcall fn (foo 2))))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(flet ((bar (x) x)) (let ((fn (lambda (foo) (bar 1)))) (funcall fn (bar 2))))"
        ]
    );
}

#[test]
fn nested_flet_definition_body_can_still_reference_outer_function() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) x)) (flet ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(flet ((bar (x) x))",
            "(flet ((foo (y) (bar y))) (foo 1))",
            "(bar 2)"
        ]
    );
}

#[test]
fn sibling_labels_definition_body_renames_same_form_references() {
    assert_local_function_rename!(
        input: "(labels ((foo (x) x) (helper (y) (list (foo y) #'foo (function foo) foo))) (helper 1) (foo 2) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 4,
        changed: true,
        rewritten_contains: [
            "(labels ((bar (x) x)",
            "(helper (y) (list (bar y) #'bar (function bar) foo))",
            "(helper 1) (bar 2) foo)"
        ]
    );
}
