use super::super::*;

#[test]
fn renames_define_compiler_macro_definition_and_designators() {
    assert_function_rename! {
        input: "(define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(list #'fast-add (function fast-add) (compiler-macro-function 'fast-add) fast-add)",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(define-compiler-macro optimized-add (x y)",
            "#'optimized-add",
            "(function optimized-add)",
            "(compiler-macro-function 'optimized-add)",
            " fast-add)"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_define_compiler_macro_definition_and_designators() {
    assert_function_rename! {
        input: "(cl:define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(list #'fast-add (function fast-add) fast-add)",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(cl:define-compiler-macro optimized-add (x y)",
            "#'optimized-add",
            "(function optimized-add)",
            " fast-add)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_define_compiler_macro_definition_and_designators() {
    assert_function_rename! {
        input: "(cl-user:define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(list #'fast-add (function fast-add) fast-add)",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(cl-user:define-compiler-macro optimized-add (x y)",
            "#'optimized-add",
            "(function optimized-add)",
            " fast-add)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_define_compiler_macro_definition_inside_reader_quoted_lambda_body_without_touching_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(cl-user:define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(defun caller () #'(lambda () (compiler-macrolet ((fast-add (value) (list #'fast-add (function fast-add) (fast-add value)))) (fast-add 1))))",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:define-compiler-macro optimized-add (x y) `(+ ,x ,y))",
            "#'(lambda () (compiler-macrolet ((fast-add (value) (list #'optimized-add (function optimized-add) (optimized-add value)))) (fast-add 1)))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_define_compiler_macro_definition_inside_reader_quoted_lambda_body_without_touching_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(cl:define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(defun caller () #'(lambda () (compiler-macrolet ((fast-add (value) (list #'fast-add (function fast-add) (fast-add value)))) (fast-add 1))))",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:define-compiler-macro optimized-add (x y) `(+ ,x ,y))",
            "#'(lambda () (compiler-macrolet ((fast-add (value) (list #'optimized-add (function optimized-add) (optimized-add value)))) (fast-add 1)))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_define_compiler_macro_definition_inside_reader_quoted_lambda_body_without_touching_qualified_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(cl:define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(defun caller () #'(lambda () (cl:compiler-macrolet ((fast-add (value) (list #'fast-add (function fast-add) (fast-add value)))) (fast-add 1))))",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:define-compiler-macro optimized-add (x y) `(+ ,x ,y))",
            "#'(lambda () (cl:compiler-macrolet ((fast-add (value) (list #'optimized-add (function optimized-add) (optimized-add value)))) (fast-add 1)))"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_define_compiler_macro_definition_inside_reader_quoted_lambda_body_without_touching_qualified_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(cl-user:define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(defun caller () #'(lambda () (cl-user:compiler-macrolet ((fast-add (value) (list #'fast-add (function fast-add) (fast-add value)))) (fast-add 1))))",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:define-compiler-macro optimized-add (x y) `(+ ,x ,y))",
            "#'(lambda () (cl-user:compiler-macrolet ((fast-add (value) (list #'optimized-add (function optimized-add) (optimized-add value)))) (fast-add 1)))"
        ]
    };
}

#[test]
fn renames_define_compiler_macro_definition_inside_reader_quoted_lambda_body_without_touching_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(define-compiler-macro fast-add (x y) `(+ ,x ,y))\n(defun caller () #'(lambda () (compiler-macrolet ((fast-add (value) (list #'fast-add (function fast-add) (fast-add value)))) (fast-add 1))))",
        dialect: Dialect::CommonLisp,
        from: "fast-add",
        to: "optimized-add",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(define-compiler-macro optimized-add (x y) `(+ ,x ,y))",
            "#'(lambda () (compiler-macrolet ((fast-add (value) (list #'optimized-add (function optimized-add) (optimized-add value)))) (fast-add 1)))"
        ]
    };
}
