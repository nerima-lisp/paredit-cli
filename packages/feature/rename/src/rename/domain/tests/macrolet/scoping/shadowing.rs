use super::super::*;

#[test]
fn renames_outer_macrolet_calls_inside_same_name_nested_expander_body() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (macrolet ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "(macrolet ((foo (y) (bar y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn renames_outer_compiler_macrolet_calls_inside_same_name_nested_expander_body() {
    assert_macrolet_rename! {
        input: "(compiler-macrolet ((foo (x) x)) (compiler-macrolet ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(compiler-macrolet ((bar (x) x))",
            "(compiler-macrolet ((foo (y) (bar y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn renames_outer_macrolet_across_package_qualified_nested_macrolet_shadowing() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (macrolet ((cl-user:foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "(macrolet ((cl-user:foo (y) (bar y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn skips_flet_calls_that_shadow_macrolet_binding() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (flet ((foo (y) y)) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "(flet ((foo (y) y)) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn skips_labels_calls_that_shadow_macrolet_binding() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (labels ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "(labels ((foo (y) (foo y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn escaped_colon_does_not_bypass_flet_shadowing() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo\\:bar (x) x)) (flet ((foo\\:bar (y) y)) (foo\\:bar 1)) (foo\\:bar 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo\\:bar",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((renamed (x) x))",
            "(flet ((foo\\:bar (y) y)) (foo\\:bar 1))",
            "(renamed 2)"
        ]
    };
}

#[test]
fn plans_qualified_macrolet_rename_without_crossing_qualified_labels_shadow() {
    assert_macrolet_rename! {
        input: "(cl:macrolet ((foo (x) x)) (cl:labels ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(cl:macrolet ((bar (x) x))",
            "(cl:labels ((foo (y) (foo y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn plans_cl_user_qualified_macrolet_rename_without_crossing_cl_user_qualified_labels_shadow() {
    assert_macrolet_rename! {
        input: "(cl-user:macrolet ((foo (x) x)) (cl-user:labels ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(cl-user:macrolet ((bar (x) x))",
            "(cl-user:labels ((foo (y) (foo y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn plans_emacs_lisp_cl_macrolet_rename_without_crossing_cl_labels_shadow() {
    assert_macrolet_rename! {
        input: "(cl-macrolet ((foo (x) x)) (cl-labels ((foo (y) (foo y))) (foo 1)) (foo 2))\n",
        dialect: Dialect::EmacsLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(cl-macrolet ((bar (x) x))",
            "(cl-labels ((foo (y) (foo y))) (foo 1))",
            "(bar 2)"
        ]
    };
}

#[test]
fn renames_outer_macrolet_calls_inside_nested_expander_body() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (macrolet ((bar (y) (foo y))) (bar 1)) (foo 2))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "baz",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(macrolet ((baz (x) x))",
            "(macrolet ((bar (y) (baz y))) (bar 1))",
            "(baz 2)"
        ]
    };
}

#[test]
fn renames_independent_macrolet_inside_expander_body() {
    assert_macrolet_rename! {
        input: "(macrolet ((outer (x) (macrolet ((foo (y) y)) (foo x)))) (outer 1))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((outer (x) (macrolet ((bar (y) y)) (bar x)))) (outer 1))"
        ]
    };
}

#[test]
fn skips_same_form_macrolet_sibling_expander_references() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x) (helper (y) (list (foo y) foo))) (helper 1) (foo 2) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x)",
            "(helper (y) (list (foo y) foo))",
            "(helper 1) (bar 2) foo)"
        ]
    };
}

#[test]
fn renames_macrolet_calls_without_touching_nested_symbol_macrolet_shadowing() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) (symbol-macrolet ((foo other)) foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "(symbol-macrolet ((foo other)) foo)",
            "(bar 1)"
        ]
    };
}

#[test]
fn renames_macrolet_calls_without_touching_qualified_symbol_macrolet_shadowing() {
    assert_macrolet_rename! {
        input: "(cl:macrolet ((foo (x) x)) (cl:symbol-macrolet ((foo other)) foo) (cl-user:symbol-macrolet ((foo other)) foo) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(cl:macrolet ((bar (x) x))",
            "(cl:symbol-macrolet ((foo other)) foo)",
            "(cl-user:symbol-macrolet ((foo other)) foo)",
            "(bar 1)"
        ]
    };
}
