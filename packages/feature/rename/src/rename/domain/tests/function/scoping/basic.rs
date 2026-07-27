use super::super::*;

#[test]
fn plans_function_rename_without_value_references() {
    assert_function_rename! {
        input: "(defun foo (x) (list foo x))\n(defun caller () (foo 1))",
        dialect: Dialect::CommonLisp,
        from: "foo",
        to: "baz",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(defun baz (x)", "(baz 1)", "(list foo x)"]
    };
}

#[test]
fn renames_function_calls_inside_bare_lambda_bodies_without_touching_shadowing_parameter() {
    assert_function_rename! {
        input: "(defun helper (v) (+ v 1))\n(defun main () (let ((fn (lambda (helper) (helper 1)))) (funcall fn (helper 2))))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(defun renamed (v) (+ v 1))",
            "(lambda (helper) (renamed 1))",
            "(funcall fn (renamed 2))"
        ]
    };
}

#[test]
fn skips_labels_local_function_calls_when_renaming_function() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun main () (labels ((helper (x) (helper x))) (helper 1)))\n(defun caller () (helper 2))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x)",
            "(labels ((helper (x) (helper x))) (helper 1))",
            "(defun caller () (renamed 2))"
        ]
    };
}

#[test]
fn renames_outer_function_calls_inside_flet_binding_bodies_only() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun main () (flet ((helper (x) (helper x))) (helper 1)))",
        dialect: Dialect::CommonLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: ["(flet ((helper (x) (renamed x))) (helper 1))"]
    };
}

#[test]
fn skips_package_qualified_flet_local_function_calls() {
    assert_function_rename! {
        input: "(defun cl-user:helper (x) x)\n(defun main () (flet ((cl-user:helper (x) (cl-user:helper x))) (cl-user:helper 1)))\n(defun caller () (cl-user:helper 2))",
        dialect: Dialect::CommonLisp,
        from: "cl-user:helper",
        to: "cl-user:renamed",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(defun cl-user:renamed (x)",
            "(flet ((cl-user:helper (x) (cl-user:renamed x))) (cl-user:helper 1))",
            "(defun caller () (cl-user:renamed 2))"
        ]
    };
}

#[test]
fn skips_package_qualified_labels_local_function_calls() {
    assert_function_rename! {
        input: "(defun cl-user:helper (x) x)\n(defun main () (labels ((cl-user:helper (x) (cl-user:helper x))) (cl-user:helper 1)))\n(defun caller () (cl-user:helper 2))",
        dialect: Dialect::CommonLisp,
        from: "cl-user:helper",
        to: "cl-user:renamed",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun cl-user:renamed (x)",
            "(labels ((cl-user:helper (x) (cl-user:helper x))) (cl-user:helper 1))",
            "(defun caller () (cl-user:renamed 2))"
        ]
    };
}

#[test]
fn renames_emacs_lisp_function_calls_and_designators_without_value_references() {
    assert_function_rename! {
        input: "(defun helper (x) x)\n(defun caller () (helper 1) #'helper (function helper) helper)",
        dialect: Dialect::EmacsLisp,
        from: "helper",
        to: "renamed",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defun renamed (x) x)",
            "(defun caller () (renamed 1) #'renamed (function renamed) helper)"
        ]
    };
}

#[test]
fn preserves_depth_first_call_order_and_exact_paths() {
    let plan = plan_rename_function(RenameFunctionRequest {
        input: "(progn (foo) (wrapper (foo) (foo)) #'foo)",
        dialect: Dialect::CommonLisp,
        from: SymbolName::new("foo").unwrap(),
        to: SymbolName::new("renamed").unwrap(),
    })
    .unwrap();

    let paths = plan
        .calls
        .iter()
        .map(|occurrence| occurrence.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["0.1.0", "0.2.1.0", "0.2.2.0", "0.3"]);
}

#[test]
fn traverses_more_than_five_thousand_nested_normal_lists() {
    const DEPTH: usize = 5_100;

    let input = format!("{}(foo){}", "(progn ".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).unwrap();
    let calls = collect_function_call_head_renames(
        &tree,
        Dialect::CommonLisp,
        &SymbolName::new("foo").unwrap(),
        &SymbolName::new("renamed").unwrap(),
    )
    .unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].text, "foo");
    assert_eq!(calls[0].path.as_str(), format!("0{}.0", ".1".repeat(DEPTH)));
}

#[test]
fn traverses_deep_local_callable_nesting_without_accumulating_scope_names() {
    const DEPTH: usize = 5_100;

    let mut input = String::with_capacity(DEPTH * 27);
    for _ in 0..DEPTH {
        input.push_str("(flet ((local () nil)) ");
    }
    input.push_str("(helper)");
    input.extend(std::iter::repeat_n(')', DEPTH));

    let tree = SyntaxTree::parse(&input).unwrap();
    let calls = collect_function_call_head_renames(
        &tree,
        Dialect::CommonLisp,
        &SymbolName::new("helper").unwrap(),
        &SymbolName::new("renamed").unwrap(),
    )
    .unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].text, "helper");
}

#[test]
fn traverses_ten_thousand_sibling_calls_in_source_order() {
    const WIDTH: usize = 10_000;

    let input = format!("(progn {})", "(foo) ".repeat(WIDTH));
    let tree = SyntaxTree::parse(&input).unwrap();
    let calls = collect_function_call_head_renames(
        &tree,
        Dialect::CommonLisp,
        &SymbolName::new("foo").unwrap(),
        &SymbolName::new("renamed").unwrap(),
    )
    .unwrap();

    assert_eq!(calls.len(), WIDTH);
    assert_eq!(calls.first().unwrap().path.as_str(), "0.1.0");
    assert_eq!(calls.last().unwrap().path.as_str(), format!("0.{WIDTH}.0"));
}
