use super::{target, *};

#[test]
fn plans_inline_let_without_touching_shadowed_lambda_parameter() {
    let input = "(let ((x 1)) (list x (lambda (x) x)))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: true,
    })
    .expect("plan");

    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(list 1 (lambda (x) x))");
}

#[test]
fn rejects_only_shadowed_lambda_references_as_unused() {
    let input = "(let ((x 1)) (lambda (x) x))";
    let error = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect_err("unused binding");

    assert!(error.to_string().contains("drop an unused binding value"));
}

#[test]
fn plans_inline_let_without_touching_shadowed_inner_let() {
    let input = "(let ((x 1)) (list x (let ((x 2)) x)))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: true,
    })
    .expect("plan");

    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(list 1 (let ((x 2)) x))");
}

#[test]
fn plans_inline_let_without_touching_package_qualified_macrolet_parameter() {
    let input = "(let ((product 1)) (+ product (cl:macrolet ((with-product (product) (* width height))) product)))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: true,
    })
    .expect("plan");

    assert_eq!(plan.reference_count, 2);
    assert_eq!(
        plan.rewritten,
        "(+ 1 (cl:macrolet ((with-product (product) (* width height))) 1))"
    );
}

#[test]
fn plans_inline_let_without_touching_package_qualified_compiler_macrolet_parameter() {
    let input = "(let ((product 1)) (+ product (cl-user:compiler-macrolet ((with-product (product) (* width height))) product)))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: true,
    })
    .expect("plan");

    assert_eq!(plan.reference_count, 2);
    assert_eq!(
        plan.rewritten,
        "(+ 1 (cl-user:compiler-macrolet ((with-product (product) (* width height))) 1))"
    );
}

#[test]
fn rejects_inline_let_that_would_capture_binding_value_free_variable() {
    let input = "(let ((y x)) (let ((x 99)) y))";
    let error = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: true,
    })
    .expect_err("capture must be refused");

    assert!(error.to_string().contains("capture variable `x`"));
}

#[test]
fn rejects_inline_let_when_compound_value_free_variable_is_shadowed() {
    let input = "(let ((y (+ a 1))) (let ((a 9)) y))";
    let error = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: true,
    })
    .expect_err("capture must be refused");

    assert!(error.to_string().contains("capture variable `a`"));
}

#[test]
fn plans_inline_let_when_binding_value_free_variable_is_not_shadowed() {
    let input = "(let ((y x)) (+ y 1))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(+ x 1)");
}

#[test]
fn plans_inline_let_when_value_reuses_binding_name_from_outer_scope() {
    let input = "(let ((x (f x))) (+ x 1))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.rewritten, "(+ (f x) 1)");
}

#[test]
fn plans_clojure_vector_inline_let_without_touching_shadowed_fn_parameter() {
    let input = "(let [x 1] (list x (fn [x] x)))";
    let plan = plan_inline_let(InlineLetRequest {
        input,
        dialect: Dialect::Clojure,
        path: None,
        target: target(input),
        allow_duplicate_evaluation: false,
    })
    .expect("plan");

    assert_eq!(plan.reference_count, 1);
    assert_eq!(plan.rewritten, "(list 1 (fn [x] x))");
}
