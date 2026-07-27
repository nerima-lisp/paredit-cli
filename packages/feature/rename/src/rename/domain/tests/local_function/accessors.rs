use super::*;

#[test]
fn plans_flet_rename_without_touching_global_function_cell_accessors() {
    assert_local_function_rename!(
        input: "(flet ((foo (x) x)) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(flet ((bar (x) x)) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (bar 1) foo)"]
    );
}

#[test]
fn plans_labels_rename_without_touching_global_function_cell_accessors() {
    assert_local_function_rename!(
        input: "(labels ((foo (x) (foo x))) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(labels ((bar (x) (bar x))) (macro-function 'foo) (compiler-macro-function 'foo) (symbol-function 'foo) (fdefinition 'foo) (bar 1) foo)"]
    );
}
