use super::*;

#[test]
fn plans_flet_rename_without_touching_definition_body_references() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(flet ((bar (x) (foo x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_labels_rename_with_recursive_definition_body_references() {
    assert_local_function_rename!(
        input: "(labels ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(labels ((bar (x) (bar x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_package_qualified_flet_rename() {
    assert_local_function_rename!(
        input: "(cl:flet ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl:flet ((bar (x) (foo x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_cl_user_qualified_flet_rename_without_touching_definition_body_references() {
    assert_local_function_rename!(
        input: "(cl-user:flet ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl-user:flet ((bar (x) (foo x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_package_qualified_labels_rename_with_recursive_definition_body_references() {
    assert_local_function_rename!(
        input: "(cl-user:labels ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(cl-user:labels ((bar (x) (bar x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_emacs_lisp_cl_flet_rename_without_touching_definition_body_references() {
    assert_local_function_rename!(
        input: "(cl-flet ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::EmacsLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl-flet ((bar (x) (foo x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_emacs_lisp_cl_labels_rename_with_recursive_definition_body_references() {
    assert_local_function_rename!(
        input: "(cl-labels ((foo (x) (foo x))) (foo 1) foo)\n",
        dialect: Dialect::EmacsLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(cl-labels ((bar (x) (bar x))) (bar 1) foo)"]
    );
}

#[test]
fn plans_flet_rename_updates_body_designators_but_not_definition_body_designators() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) #'foo (function foo) (foo x))) #'foo (function foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(flet ((bar (x) #'foo (function foo) (foo x))) #'bar (function bar) (bar 1) foo)"
        ]
    );
}

#[test]
fn plans_labels_rename_updates_recursive_and_body_designators() {
    assert_local_function_rename!(
        input: "(labels ((foo (x) #'foo (function foo) (foo x))) #'foo (function foo) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(labels ((bar (x) #'bar (function bar) (bar x))) #'bar (function bar) foo)"
        ]
    );
}

#[test]
fn plans_setf_local_callable_rename_updates_definition_and_body_calls() {
    assert_local_function_rename!(
        input: "(flet (((setf foo) (value object) value)) ((setf foo) 1 thing) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(flet (((setf bar) (value object) value)) ((setf bar) 1 thing) foo)"]
    );
}

#[test]
fn plans_labels_setf_local_callable_rename_updates_definition_and_body_calls() {
    assert_local_function_rename!(
        input: "(labels (((setf foo) (value object) value)) ((setf foo) 1 thing) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(labels (((setf bar) (value object) value)) ((setf bar) 1 thing) foo)"]
    );
}

#[test]
fn plans_flet_setf_local_callable_reader_designators() {
    assert_local_function_rename!(
        input: "(flet (((setf foo) (value object) value)) #'(setf foo) (function (setf foo)) ((setf foo) 1 thing) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(flet (((setf bar) (value object) value)) #'(setf bar) (function (setf bar)) ((setf bar) 1 thing) foo)"
        ]
    );
}

#[test]
fn plans_labels_setf_local_callable_reader_designators_and_recursion() {
    assert_local_function_rename!(
        input: "(labels (((setf foo) (value object) #'(setf foo) (function (setf foo)) ((setf foo) value object))) #'(setf foo) (function (setf foo)) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(labels (((setf bar) (value object) #'(setf bar) (function (setf bar)) ((setf bar) value object))) #'(setf bar) (function (setf bar)) foo)"
        ]
    );
}
