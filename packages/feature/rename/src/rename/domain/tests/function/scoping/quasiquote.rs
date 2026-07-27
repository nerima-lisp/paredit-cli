use super::super::*;

#[test]
fn preserves_unquote_prefixes_when_renaming_function_calls_inside_quasiquote() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defmacro build () `(list ,(helper 1) ,@(helper 2) (helper 3)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: ["(defun renamed (x) x)", "`(list ,(renamed 1) ,@(renamed 2) (renamed 3))"]
    };
}

#[test]
fn renames_function_designators_but_skips_quoted_data() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(list '(helper 1) #'helper (function helper) `(helper ,value) (helper 2) (symbol-function 'helper) (fdefinition 'helper))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "'(helper 1)",
            "#'renamed",
            "(function renamed)",
            "`(helper ,value)",
            "(renamed 2)",
            "(symbol-function 'renamed)",
            "(fdefinition 'renamed)"
        ]
    };
}

#[test]
fn preserves_quoted_data_while_renaming_calls_and_explicit_function_designators() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(helper 1)\n(list 'helper (quote helper) (error 'helper) (typep x 'helper) (make-instance 'helper) #'helper (function helper) (symbol-function 'helper) (fdefinition 'helper))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(renamed 1)",
            "(list 'helper (quote helper) (error 'helper) (typep x 'helper) (make-instance 'helper) #'renamed (function renamed) (symbol-function 'renamed) (fdefinition 'renamed))"
        ]
    };
}

#[test]
fn renames_unquoted_callable_designators_inside_quasiquote() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () `(list ,#'helper ,(function helper) ,(symbol-function 'helper) ,(fdefinition 'helper)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 4,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(defun caller () `(list ,#'renamed ,(function renamed) ,(symbol-function 'renamed) ,(fdefinition 'renamed)))"
        ]
    };
}

#[test]
fn renames_unquoted_macro_function_designators_inside_quasiquote() {
    assert_function_rename! {
        input: "(defmacro helper (x) x)\n(defun caller () `(list ,(macro-function 'helper) ,(macro-function 'other) '(macro-function helper)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defmacro renamed (x) x)",
            "(defun caller () `(list ,(macro-function 'renamed) ,(macro-function 'other) '(macro-function helper)))"
        ]
    };
}

#[test]
fn renames_unquoted_compiler_macro_function_designators_inside_quasiquote() {
    assert_function_rename! {
        input: "(define-compiler-macro helper (x) x)\n(defun caller () `(list ,(compiler-macro-function 'helper) ,(compiler-macro-function 'other) '(compiler-macro-function helper)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(define-compiler-macro renamed (x) x)",
            "(defun caller () `(list ,(compiler-macro-function 'renamed) ,(compiler-macro-function 'other) '(compiler-macro-function helper)))"
        ]
    };
}

#[test]
fn renames_qualified_macro_function_designators_inside_quasiquote() {
    assert_function_rename! {
        input: "(defmacro helper (x) x)\n(define-compiler-macro helper (x) x)\n(defun caller () `(list ,(cl:macro-function 'cl-user:helper) ,(cl-user:compiler-macro-function 'common-lisp-user:helper) '(cl:macro-function helper) '(cl-user:compiler-macro-function helper)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 2,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(defmacro renamed (x) x)",
            "(define-compiler-macro renamed (x) x)",
            "(defun caller () `(list ,(cl:macro-function 'renamed) ,(cl-user:compiler-macro-function 'renamed) '(cl:macro-function helper) '(cl-user:compiler-macro-function helper)))"
        ]
    };
}
