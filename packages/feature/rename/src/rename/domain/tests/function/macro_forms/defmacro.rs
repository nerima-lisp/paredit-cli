use super::super::*;

#[test]
fn renames_defmacro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(defmacro helper (x) `(list ,x))\n(helper 1)\n(list #'helper (macro-function 'helper) helper '(helper 2))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defmacro renamed (x)",
            "(renamed 1)",
            "#'renamed",
            "(macro-function 'renamed)",
            "helper '(helper 2)"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_defmacro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(cl:defmacro helper (x) `(list ,x))\n(helper 1)\n(list #'helper helper)",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(cl:defmacro renamed (x)", "(renamed 1)", "#'renamed helper"]
    };
}

#[test]
fn renames_common_lisp_user_qualified_defmacro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(cl-user:defmacro helper (x) `(list ,x))\n(helper 1)\n(list #'helper helper)",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(cl-user:defmacro renamed (x)", "(renamed 1)", "#'renamed helper"]
    };
}

#[test]
fn renames_emacs_lisp_cl_defmacro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(cl-defmacro helper (x) `(list ,x))\n(helper 1)\n(defun caller () (helper 2) helper)",
        dialect: Dialect::EmacsLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(cl-defmacro renamed (x)",
            "(renamed 1)",
            "(defun caller () (renamed 2) helper)"
        ]
    };
}

#[test]
fn renames_defmacro_definition_inside_reader_quoted_lambda_body_without_touching_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(defmacro helper (x) `(list ,x))\n(defun caller () #'(lambda () (macrolet ((helper (value) (list #'helper (function helper) (helper value)))) (helper 1))))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defmacro renamed (x) `(list ,x))",
            "#'(lambda () (macrolet ((helper (value) (list #'renamed (function renamed) (renamed value)))) (helper 1)))"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_macrolet_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(defun caller () (macrolet ((local (value) `(helper ,value))) (local 1)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(macrolet ((local (value) `(renamed ,value))) (local 1))"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_compiler_macrolet_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(defun caller () (compiler-macrolet ((local (value) `(helper ,value))) (local 1)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(compiler-macrolet ((local (value) `(renamed ,value))) (local 1))"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_defmacro_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(defmacro local (value) `(helper ,value))\n(local 1)",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(defmacro local (value) `(renamed ,value))"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_emacs_lisp_cl_defmacro_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(cl-defmacro local (value) `(helper ,value))\n(local 1)",
        dialect: Dialect::EmacsLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(cl-defmacro local (value) `(renamed ,value))"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_define_compiler_macro_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(define-compiler-macro local (value) `(helper ,value))\n(local 1)",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(define-compiler-macro local (value) `(renamed ,value))"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_define_setf_expander_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(define-setf-expander slot (place) (values nil nil nil `(helper store) `(helper ,place)))\n(helper root)",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(define-setf-expander slot (place) (values nil nil nil `(renamed store) `(renamed ,place)))",
            "(renamed root)"
        ]
    };
}

#[test]
fn renames_function_designators_generated_by_macrolet_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(defun caller () (macrolet ((local () `(#'helper (function helper) (macro-function 'helper) (compiler-macro-function 'helper) (symbol-function 'helper) (fdefinition 'helper)))) (local)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 6,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(macrolet ((local () `(#'renamed (function renamed) (macro-function 'renamed) (compiler-macro-function 'renamed) (symbol-function 'renamed) (fdefinition 'renamed)))) (local))"
        ]
    };
}

#[test]
fn does_not_rename_function_calls_inside_flet_quasiquoted_data() {
    assert_function_rename! {
        input: "(defun helper (value) value)\n(defun caller () (flet ((local (value) `(helper ,value))) (local 1)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 0,
        changed: true,
        rewritten_contains: [
            "(defun renamed (value) value)",
            "(flet ((local (value) `(helper ,value))) (local 1))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_defmacro_definition_inside_reader_quoted_lambda_body_without_touching_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(cl:defmacro helper (x) `(list ,x))\n(defun caller () #'(lambda () (macrolet ((helper (value) (list #'helper (function helper) (helper value)))) (helper 1))))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:defmacro renamed (x) `(list ,x))",
            "#'(lambda () (macrolet ((helper (value) (list #'renamed (function renamed) (renamed value)))) (helper 1)))"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_defmacro_definition_inside_reader_quoted_lambda_body_without_touching_shadowed_macro_body()
 {
    assert_function_rename! {
        input: "(cl-user:defmacro helper (x) `(list ,x))\n(defun caller () #'(lambda () (macrolet ((helper (value) (list #'helper (function helper) (helper value)))) (helper 1))))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:defmacro renamed (x) `(list ,x))",
            "#'(lambda () (macrolet ((helper (value) (list #'renamed (function renamed) (renamed value)))) (helper 1)))"
        ]
    };
}
