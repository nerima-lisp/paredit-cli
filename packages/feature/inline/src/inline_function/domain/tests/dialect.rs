use super::super::calls::inline_function_symbol_reference_eq;
use super::*;

#[test]
fn common_lisp_reader_collision_output_reparses_with_the_same_dialect() {
    let input = "(defun identity (value) value)\n(print (identity #\\)))";
    let plan = inline_plan(input);

    assert_eq!(
        plan.rewritten,
        "(defun identity (value) value)\n(print #\\))"
    );
    SyntaxTree::parse_with_dialect(&plan.rewritten, plan.dialect).expect("same-dialect reparse");
}

#[test]
fn unsupported_dialects_fail_before_malformed_input_is_parsed() {
    for dialect in [
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ] {
        let error = plan_inline_function(InlineFunctionRequest {
            input: "(",
            dialect,
            definition_path: path("0"),
            call_paths: vec![path("1")],
            all_calls: false,
            remove_definition: false,
            allow_duplicate_evaluation: false,
            allow_drop_arguments: false,
        })
        .expect_err("unsupported dialect");

        assert_eq!(
            error.to_string(),
            format!(
                "inline-function does not support dialect {}",
                dialect.label()
            )
        );
    }
}

#[test]
fn emacs_lisp_macro_definitions_remain_unsupported() {
    let error = plan_inline_function(InlineFunctionRequest {
        input: "(defmacro helper (x) `(+ ,x 1))\n(helper 2)",
        dialect: Dialect::EmacsLisp,
        definition_path: path("0"),
        call_paths: vec![path("1")],
        all_calls: false,
        remove_definition: false,
        allow_duplicate_evaluation: false,
        allow_drop_arguments: false,
    })
    .expect_err("Emacs Lisp macro definitions stay outside inline-function support");

    assert_eq!(
        error.to_string(),
        "inline-function does not support definition head: defmacro"
    );
}

#[test]
fn function_reference_equality_is_dialect_aware_and_unknown_fails_closed() {
    assert!(inline_function_symbol_reference_eq(
        Dialect::CommonLisp,
        "IDENTITY",
        "identity"
    ));

    for dialect in [
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
    ] {
        assert!(inline_function_symbol_reference_eq(
            dialect, "identity", "identity"
        ));
        assert!(!inline_function_symbol_reference_eq(
            dialect, "IDENTITY", "identity"
        ));
    }

    assert!(!inline_function_symbol_reference_eq(
        Dialect::Unknown,
        "identity",
        "identity"
    ));
}
