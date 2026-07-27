use super::*;

#[test]
fn preserves_unquote_prefixes_when_renaming_macrolet_calls_inside_quasiquote() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) `(list ,(foo 1) ,@(foo 2) (foo 3)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "`(list ,(bar 1) ,@(bar 2) (foo 3)))"
        ]
    };
}

#[test]
fn skips_macrolet_definitions_and_calls_inside_quoted_data() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) x)) '(macrolet ((foo (y) y)) (foo 1)) `(progn (macrolet ((foo (z) z)) (foo 2)) ,(foo 3)))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) x))",
            "'(macrolet ((foo (y) y)) (foo 1)) `(progn (macrolet ((foo (z) z)) (foo 2)) ,(bar 3))"
        ]
    };
}
