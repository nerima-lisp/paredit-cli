use super::super::*;

#[test]
fn renames_function_calls_inside_reader_quoted_lambda_bodies() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller (x) #'(lambda (y) (helper y)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["#'(lambda (y) (renamed y))"]
    };
}

#[test]
fn skips_non_lambda_lists_inside_reader_quoted_function_prefixes() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () #'(foo (helper 1)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 0,
        changed: true,
        rewritten_contains: ["#'(foo (helper 1))"]
    };
}

#[test]
fn skips_reader_eval_bodies_when_renaming_functions() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () #.(list helper #'helper (function helper) (helper 1)) (helper 2))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(defun caller () #.(list helper #'helper (function helper) (helper 1)) (renamed 2))"
        ]
    };
}
