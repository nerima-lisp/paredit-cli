use super::super::*;

#[test]
fn renames_outer_function_without_touching_nested_symbol_macrolet_shadowing() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () (symbol-macrolet ((helper other)) helper #'helper (function helper) (helper 1)) (helper 2) #'helper (function helper) (helper 3))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 7,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(symbol-macrolet ((helper other)) helper #'renamed (function renamed) (renamed 1))",
            "(renamed 2) #'renamed (function renamed) (renamed 3)"
        ]
    };
}

#[test]
fn renames_outer_function_without_touching_qualified_symbol_macrolet_shadowing() {
    assert_function_rename! {
        input: "(cl:defun helper (x) x)\n(defun caller () (cl:symbol-macrolet ((helper other)) helper #'helper (function helper) (helper 1)) (cl-user:symbol-macrolet ((helper other)) helper #'helper (function helper) (helper 2)) (helper 3) #'helper (function helper) (helper 4))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 10,
        changed: true,
        rewritten_contains: [
            "(cl:defun renamed (x) x)",
            "(cl:symbol-macrolet ((helper other)) helper #'renamed (function renamed) (renamed 1))",
            "(cl-user:symbol-macrolet ((helper other)) helper #'renamed (function renamed) (renamed 2))",
            "(renamed 3) #'renamed (function renamed) (renamed 4)"
        ]
    };
}

#[test]
fn renames_outer_function_without_touching_package_qualified_symbol_macrolet_binding_names() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () (symbol-macrolet ((cl-user:helper other)) helper #'helper (function helper) (helper 1)) (helper 2) #'helper (function helper) (helper 3))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 7,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(symbol-macrolet ((cl-user:helper other)) helper #'renamed (function renamed) (renamed 1))",
            "(renamed 2) #'renamed (function renamed) (renamed 3)"
        ]
    };
}

#[test]
fn renames_function_calls_inside_symbol_macrolet_expansion_forms() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () (symbol-macrolet ((helper (helper 0))) helper #'helper (function helper) (helper 1)) (helper 2))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(symbol-macrolet ((helper (renamed 0))) helper #'renamed (function renamed) (renamed 1))",
            "(renamed 2)"
        ]
    };
}

#[test]
fn renames_function_calls_inside_package_qualified_symbol_macrolet_expansion_forms() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () (symbol-macrolet ((cl-user:helper (helper 0))) helper #'helper (function helper) (helper 1)) (helper 2) #'helper (function helper) (helper 3))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 8,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(symbol-macrolet ((cl-user:helper (renamed 0))) helper #'renamed (function renamed) (renamed 1))",
            "(renamed 2) #'renamed (function renamed) (renamed 3)"
        ]
    };
}

#[test]
fn renames_outer_function_references_inside_reader_quoted_lambda_bodies_only() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () #'(lambda () (symbol-macrolet ((helper other)) helper) helper #'helper (function helper) (helper 1)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "#'(lambda () (symbol-macrolet ((helper other)) helper) helper #'renamed (function renamed) (renamed 1))"
        ]
    };
}

#[test]
fn renames_package_qualified_function_references_inside_symbol_macrolet_bodies() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () (symbol-macrolet ((helper other)) helper #'cl-user:helper (function cl-user:helper) (cl-user:helper 1)) #'cl-user:helper (function cl-user:helper) (cl-user:helper 2))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 6,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(symbol-macrolet ((helper other)) helper #'renamed (function renamed) (renamed 1))",
            "#'renamed (function renamed) (renamed 2)"
        ]
    };
}
