use proptest::prelude::*;

use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SyntaxTree;

fn reports_for(input: &str, dialect: Dialect) -> Vec<LetFormReport> {
    let tree = SyntaxTree::parse(input).expect("valid test input");
    build_let_report(dialect, input, &tree).expect("let report builds")
}

#[test]
fn reports_inlineable_single_common_lisp_let() {
    let reports = reports_for("(let ((x 1)) (+ x 2))", Dialect::CommonLisp);

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].binding_style, "list-pair");
    assert!(reports[0].inline_supported_by_inline_let);
    assert_eq!(reports[0].bindings[0].reference_count, 1);
    assert!(reports[0].bindings[0].can_inline_without_duplication);
}

#[test]
fn reports_let_star_later_binding_reference() {
    let reports = reports_for("(let* ((x 1) (y (+ x 2))) y)", Dialect::CommonLisp);

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].bindings[0].reference_count, 1);
    assert!(reports[0].bindings[0].risks.contains(&"multiple-bindings"));
}

#[test]
fn reports_clojure_vector_bindings() {
    let reports = reports_for("(let [x 1 y (+ x 2)] y)", Dialect::Clojure);

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].binding_style, "vector");
    assert_eq!(reports[0].bindings[0].reference_count, 1);
    assert_eq!(reports[0].bindings[1].reference_count, 1);
}

#[test]
fn reports_ignore_shadowed_body_references() {
    let reports = reports_for("(let ((x 1)) (let ((x 2)) x))", Dialect::CommonLisp);

    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
    assert_eq!(reports[1].bindings[0].reference_count, 1);
}

#[test]
fn reports_ignore_lambda_parameter_shadowed_references() {
    let reports = reports_for("(let ((x 1)) (lambda (x) x))", Dialect::CommonLisp);

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
}

#[test]
fn ignores_a_let_form_inside_a_quasiquoted_code_generation_template() {
    // A with-gensyms-style macro helper builds generated code via a
    // quasiquote template, e.g. `` `(let ((,x ,val)) ...) ``. That inner
    // `let` shape is not a real binding: its "name" is an unquoted gensym
    // variable determined at macro-expansion time, not a symbol whose
    // unused-ness this tool can judge. Only the outer, real `let` (which
    // executes at macro-expansion time to create the gensym) should be
    // reported.
    let input = "(let ((b (gensym)))\n  `(let ((,b ,broker)) (frob ,b)))";
    let reports = reports_for(input, Dialect::CommonLisp);

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].bindings[0].name, "b");
}

#[test]
fn still_reports_a_real_let_form_nested_inside_an_unquote() {
    // An unquoted (`,`) sub-expression inside a quasiquote template is
    // ordinary evaluated code, not generated-code data, so a `let` there is
    // real and must still be analyzed like any other.
    let input = "`(foo ,(let ((y 1)) :unused))";
    let reports = reports_for(input, Dialect::CommonLisp);

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].bindings[0].name, "y");
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
}

#[test]
fn reports_capture_risk_when_value_free_variable_is_shadowed() {
    let reports = reports_for("(let ((y x)) (let ((x 99)) y))", Dialect::CommonLisp);

    assert_eq!(reports[0].bindings[0].name, "y");
    assert_eq!(reports[0].bindings[0].reference_count, 1);
    assert!(reports[0].bindings[0].risks.contains(&"capture"));
    assert!(!reports[0].bindings[0].can_inline_without_duplication);
}

#[test]
fn reports_no_capture_risk_when_value_free_variable_is_unshadowed() {
    let reports = reports_for("(let ((y x)) (+ y 1))", Dialect::CommonLisp);

    assert!(!reports[0].bindings[0].risks.contains(&"capture"));
    assert!(reports[0].bindings[0].can_inline_without_duplication);
}

#[test]
fn reports_symbol_macrolet_without_counting_expansion_reference() {
    let reports = reports_for(
        "(symbol-macrolet ((value (compute value)) (used other)) (list used))",
        Dialect::CommonLisp,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].form, "symbol-macrolet");
    assert_eq!(reports[0].binding_style, "list-pair");
    assert!(!reports[0].inline_supported_by_inline_let);
    assert_eq!(reports[0].bindings[0].name, "value");
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
    assert!(
        reports[0].bindings[0]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert_eq!(reports[0].bindings[1].name, "used");
    assert_eq!(reports[0].bindings[1].reference_count, 1);
    assert!(
        reports[0].bindings[1]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert!(!reports[0].bindings[1].can_inline_without_duplication);
}

#[test]
fn reports_emacs_lisp_cl_symbol_macrolet_without_counting_expansion_reference() {
    let reports = reports_for(
        "(cl-symbol-macrolet ((value (compute value)) (used other)) (list used))",
        Dialect::EmacsLisp,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].form, "cl-symbol-macrolet");
    assert_eq!(reports[0].binding_style, "list-pair");
    assert!(!reports[0].inline_supported_by_inline_let);
    assert_eq!(reports[0].bindings[0].name, "value");
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
    assert!(
        reports[0].bindings[0]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert_eq!(reports[0].bindings[1].name, "used");
    assert_eq!(reports[0].bindings[1].reference_count, 1);
    assert!(
        reports[0].bindings[1]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert!(!reports[0].bindings[1].can_inline_without_duplication);
}

#[test]
fn reports_single_symbol_macrolet_as_supported_by_inline_let() {
    let reports = reports_for(
        "(symbol-macrolet ((value (compute value))) (list value))",
        Dialect::CommonLisp,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].form, "symbol-macrolet");
    assert!(reports[0].inline_supported_by_inline_let);
    assert_eq!(reports[0].bindings[0].reference_count, 1);
    assert!(
        !reports[0].bindings[0]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert!(reports[0].bindings[0].can_inline_without_duplication);
}

#[test]
fn reports_single_common_lisp_cl_user_symbol_macrolet_as_supported_by_inline_let() {
    let reports = reports_for(
        "(cl-user:symbol-macrolet ((value (compute value))) (list value))",
        Dialect::CommonLisp,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].form, "cl-user:symbol-macrolet");
    assert!(reports[0].inline_supported_by_inline_let);
    assert_eq!(reports[0].bindings[0].reference_count, 1);
    assert!(
        !reports[0].bindings[0]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert!(reports[0].bindings[0].can_inline_without_duplication);
}

#[test]
fn reports_common_lisp_cl_user_symbol_macrolet_without_counting_expansion_reference() {
    let reports = reports_for(
        "(cl-user:symbol-macrolet ((value (compute value)) (used other)) (list used))",
        Dialect::CommonLisp,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].form, "cl-user:symbol-macrolet");
    assert_eq!(reports[0].binding_style, "list-pair");
    assert!(!reports[0].inline_supported_by_inline_let);
    assert_eq!(reports[0].bindings[0].name, "value");
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
    assert!(
        reports[0].bindings[0]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert_eq!(reports[0].bindings[1].name, "used");
    assert_eq!(reports[0].bindings[1].reference_count, 1);
    assert!(
        reports[0].bindings[1]
            .risks
            .contains(&"unsupported-by-inline-let")
    );
    assert!(!reports[0].bindings[1].can_inline_without_duplication);
}

#[test]
fn reports_earmuffed_special_variable_rebind_distinctly_instead_of_unused_binding() {
    // `(let ((*read-eval* nil)) (read stream))` rebinds a special variable
    // purely for its dynamic-scope side effect; no lexical reference is
    // needed or expected. Flagging it "unused-binding" would invite
    // deleting a binding that can be load-bearing for program behavior (in
    // this exact shape, a defense against arbitrary code execution via
    // `#.` during `read`).
    let reports = reports_for(
        "(let ((*read-eval* nil)) (read stream))",
        Dialect::CommonLisp,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].bindings[0].reference_count, 0);
    assert!(
        reports[0].bindings[0]
            .risks
            .contains(&"possible-dynamic-variable-rebind")
    );
    assert!(!reports[0].bindings[0].risks.contains(&"unused-binding"));
    assert!(!reports[0].bindings[0].can_inline_without_duplication);
}

#[test]
fn reports_unused_binding_for_a_non_earmuffed_zero_reference_name() {
    // A name that merely starts or ends with `*` (not both) is not the
    // earmuff convention and must not be exempted.
    let reports = reports_for("(let ((*unused)) 42)", Dialect::CommonLisp);

    assert_eq!(reports.len(), 1);
    assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
}

#[test]
fn validates_policy_threshold() {
    assert!(LetReportPolicyOptions::new(true, true, Some(1)).is_ok());
    assert_eq!(
        LetReportPolicyOptions::new(false, false, Some(0)).unwrap_err(),
        "require-inlineable-bindings must be greater than zero"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn pbt_generated_single_binding_reference_counts_are_reported(ref_count in 0usize..4) {
        let body = match ref_count {
            0 => "42".to_owned(),
            1 => "x".to_owned(),
            n => format!("(+ {})", std::iter::repeat_n("x", n).collect::<Vec<_>>().join(" ")),
        };
        let input = format!("(let ((x 1)) {body})");
        let reports = reports_for(&input, Dialect::CommonLisp);

        prop_assert_eq!(reports.len(), 1);
        prop_assert_eq!(reports[0].bindings[0].reference_count, ref_count);
        prop_assert_eq!(
            reports[0].bindings[0].risks.contains(&"duplicate-evaluation"),
            ref_count > 1
        );
        prop_assert_eq!(
            reports[0].bindings[0].risks.contains(&"unused-binding"),
            ref_count == 0
        );
    }

    #[test]
    fn pbt_shadowed_nested_let_references_do_not_count(value in 0i64..100, shadow_value in 0i64..100) {
        let input = format!("(let ((x {value})) (let ((x {shadow_value})) (+ x x)))");
        let reports = reports_for(&input, Dialect::CommonLisp);

        prop_assert_eq!(reports.len(), 2);
        prop_assert_eq!(reports[0].bindings[0].reference_count, 0);
        prop_assert!(reports[0].bindings[0].risks.contains(&"unused-binding"));
        prop_assert_eq!(reports[1].bindings[0].reference_count, 2);
    }
}
