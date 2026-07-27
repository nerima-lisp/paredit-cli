use super::*;

#[test]
fn plans_macrolet_rename_without_touching_expander_body_references() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) (list foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(macrolet ((bar (x) (list foo x))) (bar 1) foo)"]
    };
}

#[test]
fn plans_compiler_macrolet_rename_without_touching_expander_body_references() {
    assert_macrolet_rename! {
        input: "(compiler-macrolet ((foo (x) (list foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(compiler-macrolet ((bar (x) (list foo x))) (bar 1) foo)"]
    };
}

#[test]
fn plans_cl_user_qualified_compiler_macrolet_rename_without_touching_expander_body_references() {
    assert_macrolet_rename! {
        input: "(cl-user:compiler-macrolet ((foo (x) (list foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl-user:compiler-macrolet ((bar (x) (list foo x))) (bar 1) foo)"]
    };
}

#[test]
fn plans_cl_user_qualified_macrolet_rename_without_touching_expander_body_references() {
    assert_macrolet_rename! {
        input: "(cl-user:macrolet ((foo (x) (list foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl-user:macrolet ((bar (x) (list foo x))) (bar 1) foo)"]
    };
}

#[test]
fn plans_cl_qualified_compiler_macrolet_rename_without_touching_expander_body_references() {
    assert_macrolet_rename! {
        input: "(cl:compiler-macrolet ((foo (x) (list foo x))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl:compiler-macrolet ((bar (x) (list foo x))) (bar 1) foo)"]
    };
}

#[test]
fn plans_emacs_lisp_cl_macrolet_rename_without_touching_expander_body_references() {
    assert_macrolet_rename! {
        input: "(cl-macrolet ((foo (x) (list foo x))) (foo 1) foo)\n",
        dialect: Dialect::EmacsLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(cl-macrolet ((bar (x) (list foo x))) (bar 1) foo)"]
    };
}

#[test]
fn plans_macrolet_rename_without_touching_function_designators() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) (list #'foo (function foo) x))) #'foo (function foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) (list #'foo (function foo) x))) #'foo (function foo) (bar 1) foo)"
        ]
    };
}

#[test]
fn plans_macrolet_rename_for_package_qualified_binding_name() {
    assert_macrolet_rename! {
        input: "(cl-user:macrolet ((cl-user:foo (x) (list cl-user:foo x))) (foo 1) cl-user:foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(cl-user:macrolet ((bar (x) (list cl-user:foo x))) (bar 1) cl-user:foo)"
        ]
    };
}
