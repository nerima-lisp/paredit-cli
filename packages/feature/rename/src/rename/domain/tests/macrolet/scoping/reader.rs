use super::super::*;

#[test]
fn renames_macrolet_calls_inside_reader_quoted_lambda_bodies() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) #'(lambda () (foo x) foo))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) #'(lambda () (bar x) bar))) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_macrolet_calls_inside_bare_lambda_bodies_without_touching_shadowing_parameter() {
    assert_macrolet_rename! {
        input: "(macrolet ((foo (x) `(+ ,x 1))) (let ((fn (lambda (foo) (foo 1)))) (funcall fn (foo 2))))\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(macrolet ((bar (x) `(+ ,x 1))) (let ((fn (lambda (foo) (bar 1)))) (funcall fn (bar 2))))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_macrolet_calls_inside_reader_quoted_lambda_bodies() {
    assert_macrolet_rename! {
        input: "(cl:macrolet ((foo (x) #'(lambda () (foo x) foo))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:macrolet ((bar (x) #'(lambda () (bar x) bar))) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_macrolet_calls_inside_reader_quoted_lambda_bodies() {
    assert_macrolet_rename! {
        input: "(cl-user:macrolet ((foo (x) #'(lambda () (foo x) foo))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:macrolet ((bar (x) #'(lambda () (bar x) bar))) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_compiler_macrolet_calls_inside_reader_quoted_lambda_bodies_without_touching_function_designators()
 {
    assert_macrolet_rename! {
        input: "(compiler-macrolet ((foo (x) #'(lambda () (list #'foo (function foo) (foo x) foo)))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(compiler-macrolet ((bar (x) #'(lambda () (list #'foo (function foo) (bar x) foo)))) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_compiler_macrolet_calls_inside_reader_quoted_lambda_bodies_without_touching_function_designators()
 {
    assert_macrolet_rename! {
        input: "(cl:compiler-macrolet ((foo (x) #'(lambda () (list #'foo (function foo) (foo x) foo)))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(cl:compiler-macrolet ((bar (x) #'(lambda () (list #'foo (function foo) (bar x) foo)))) (bar 1) foo)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_compiler_macrolet_calls_inside_reader_quoted_lambda_bodies_without_touching_function_designators()
 {
    assert_macrolet_rename! {
        input: "(cl-user:compiler-macrolet ((foo (x) #'(lambda () (list #'foo (function foo) (foo x) foo)))) (foo 1) foo)\n",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "bar",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(cl-user:compiler-macrolet ((bar (x) #'(lambda () (list #'foo (function foo) (bar x) foo)))) (bar 1) foo)"
        ]
    };
}
