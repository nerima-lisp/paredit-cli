use super::super::*;

#[test]
fn renames_macrolet_calls_without_touching_global_macro_cell_accessors() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (macro-function 'foo) (compiler-macro-function 'foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x)) (macro-function 'foo) (compiler-macro-function 'foo) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_compiler_macrolet_calls_without_touching_global_macro_cell_accessors() {
    assert_macrolet_rename! {
        input: "(compiler-macrolet ((foo (x) x)) (macro-function 'foo) (compiler-macro-function 'foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(compiler-macrolet ((bar (x) x)) (macro-function 'foo) (compiler-macro-function 'foo) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_macrolet_calls_without_touching_setf_function_call_heads() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) ((setf foo) 1 thing) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x)) ((setf foo) 1 thing) (bar 1) foo)"
        ]
    };
}
