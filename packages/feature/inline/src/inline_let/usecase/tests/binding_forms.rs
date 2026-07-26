use super::{target, *};

#[test]
fn plans_common_lisp_inline_let() {
    let input = "(let ((product (* width height))) (+ product margin))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_str(), "product");
    assert_eq!(plan.binding_value, "(* width height)");
    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.replacement, "(+ (* width height) margin)");
    assert_eq!(plan.rewritten, "(+ (* width height) margin)");
    assert!(plan.changed);
}

#[test]
fn plans_common_lisp_inline_let_star() {
    let input = "(let* ((product (* width height))) (+ product margin))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_str(), "product");
    assert_eq!(plan.binding_value, "(* width height)");
    assert_eq!(plan.rewritten, "(+ (* width height) margin)");
    assert!(plan.changed);
}

#[test]
fn plans_common_lisp_symbol_macrolet_inline_let() {
    let input = "(symbol-macrolet ((product (* width height))) (+ product margin))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_str(), "product");
    assert_eq!(plan.binding_value, "(* width height)");
    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(+ (* width height) margin)");
    assert!(plan.changed);
}

#[test]
fn plans_emacs_lisp_cl_symbol_macrolet_inline_let() {
    let input = "(cl-symbol-macrolet ((product (* width height))) (+ product margin))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::EmacsLisp,
        path: Some("0".parse().expect("path")),
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_str(), "product");
    assert_eq!(plan.binding_value, "(* width height)");
    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(+ (* width height) margin)");
    assert!(plan.changed);
}

#[test]
fn plans_common_lisp_cl_user_symbol_macrolet_inline_let() {
    let input = "(cl-user:symbol-macrolet ((product (* width height))) (+ product margin))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_str(), "product");
    assert_eq!(plan.binding_value, "(* width height)");
    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(+ (* width height) margin)");
    assert!(plan.changed);
}

#[test]
fn rejects_duplicate_evaluation_by_default() {
    let input = "(let ((x (compute))) (+ x x))";
    let error = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect_err("duplicate evaluation");

    assert!(error.to_string().contains("duplicate binding value"));
}

#[test]
fn plans_clojure_vector_binding() {
    let input = "(let [product (* width height)] (+ product margin))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::Clojure,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_str(), "product");
    assert_eq!(plan.rewritten, "(+ (* width height) margin)");
}
