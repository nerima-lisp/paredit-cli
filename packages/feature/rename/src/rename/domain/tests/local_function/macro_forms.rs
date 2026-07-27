use super::*;

#[test]
fn plans_outer_flet_rename_inside_macrolet_expander_but_not_shadowed_body() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) x)) (macrolet ((foo () #'foo (function foo) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (foo 1))) (foo) #'foo (function foo) (foo 2)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(flet ((bar (x) x))",
            "(macrolet ((foo () #'bar (function bar) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (bar 1))) (foo) #'foo (function foo) (foo 2)))"
        ]
    );
}

#[test]
fn plans_flet_rename_inside_reader_quoted_lambda_bodies() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) #'(lambda () (foo x) foo))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: ["(flet ((bar (x) #'(lambda () (bar x) bar))) (bar 1) foo)"]
    );
}

#[test]
fn plans_outer_flet_rename_inside_compiler_macrolet_expander_but_not_shadowed_body() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) x)) (compiler-macrolet ((foo () #'foo (function foo) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (foo 1))) (foo) #'foo (function foo) (foo 2)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(flet ((bar (x) x))",
            "(compiler-macrolet ((foo () #'bar (function bar) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (bar 1))) (foo) #'foo (function foo) (foo 2)))"
        ]
    );
}

#[test]
fn plans_outer_setf_flet_rename_inside_macrolet_expander_but_not_shadowed_body() {
    assert_local_function_rename!(
        input: "(flet (((setf foo) (value object) value)) (macrolet ((foo () #'(setf foo) (function (setf foo)) ((setf foo) 1 thing))) (foo) #'(setf foo) (function (setf foo)) ((setf foo) 2 thing)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(flet (((setf bar) (value object) value))",
            "(macrolet ((foo () #'(setf bar) (function (setf bar)) ((setf bar) 1 thing))) (foo) #'(setf foo) (function (setf foo)) ((setf foo) 2 thing)))"
        ]
    );
}

#[test]
fn plans_outer_setf_flet_rename_inside_compiler_macrolet_expander_but_not_shadowed_body() {
    assert_local_function_rename!(
        input: "(flet (((setf foo) (value object) value)) (compiler-macrolet ((foo () #'(setf foo) (function (setf foo)) ((setf foo) 1 thing))) (foo) #'(setf foo) (function (setf foo)) ((setf foo) 2 thing)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(flet (((setf bar) (value object) value))",
            "(compiler-macrolet ((foo () #'(setf bar) (function (setf bar)) ((setf bar) 1 thing))) (foo) #'(setf foo) (function (setf foo)) ((setf foo) 2 thing)))"
        ]
    );
}

#[test]
fn plans_package_qualified_outer_flet_rename_inside_compiler_macrolet_expander_but_not_shadowed_body()
 {
    assert_local_function_rename!(
        input: "(cl-user:flet ((foo (x) x)) (cl-user:compiler-macrolet ((foo () #'foo (function foo) (foo 1))) (foo) #'foo (function foo) (foo 2)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:flet ((bar (x) x))",
            "(cl-user:compiler-macrolet ((foo () #'bar (function bar) (bar 1))) (foo) #'foo (function foo) (foo 2)))"
        ]
    );
}
