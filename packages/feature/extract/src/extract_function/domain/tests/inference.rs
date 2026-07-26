use super::*;
use crate::domain::dialect::Dialect;

#[test]
fn infers_free_variables_from_selected_expression() {
    let params = infer_at(
        "(defun render (width height margin) (+ (* width height) margin))",
        &[0, 3],
        &[],
    );

    assert_eq!(params, vec!["width", "height", "margin"]);
}

#[test]
fn excludes_local_let_bindings_from_body() {
    let params = infer_at("(let ((local input)) (+ local outer))", &[0], &[]);

    assert_eq!(params, vec!["input", "outer"]);
}

#[test]
fn treats_let_star_bindings_as_sequential() {
    let params = infer_at(
        "(let* ((first input) (second first)) (+ first second outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["input", "outer"]);
}

#[test]
fn treats_symbol_macrolet_body_names_as_local() {
    let params = infer_at(
        "(symbol-macrolet ((local (compute outer))) (list local outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn treats_cl_user_symbol_macrolet_body_names_as_local() {
    let params = infer_at(
        "(cl-user:symbol-macrolet ((local (compute outer))) (list local outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn treats_package_qualified_symbol_macrolet_body_names_as_local() {
    let params = infer_at(
        "(symbol-macrolet ((cl:product (compute outer))) (list product outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn treats_destructuring_bind_names_as_local_to_body() {
    let params = infer_at(
        "(destructuring-bind (local other) row (list local other outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["row", "outer"]);
}

#[test]
fn treats_qualified_destructuring_bind_names_as_local_to_body() {
    let params = infer_at(
        "(cl:destructuring-bind (local other) row (list local other outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["row", "outer"]);
}

#[test]
fn treats_multiple_value_bind_names_as_local_to_body() {
    let params = infer_at(
        "(multiple-value-bind (value foundp) (lookup key table) (list value foundp outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["key", "table", "outer"]);
}

#[test]
fn treats_handler_case_clause_lambda_lists_as_local() {
    let params = infer_at(
        "(handler-case (risky input) (error (condition) (recover condition outer)) (:no-error (value) (finish value done)))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["input", "outer", "done"]);
}

#[test]
fn treats_qualified_handler_case_clause_lambda_lists_as_local() {
    let params = infer_at(
        "(cl:handler-case (risky input) (error (condition) (recover condition outer)) (:no-error (value) (finish value done)))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["input", "outer", "done"]);
}

#[test]
fn treats_restart_case_clause_lambda_lists_as_local() {
    let params = infer_at(
        "(restart-case (risky input) (retry (condition) (recover condition outer)) (skip () fallback))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["input", "outer", "fallback"]);
}

#[test]
fn treats_handler_bind_handler_lambda_lists_as_local() {
    let params = infer_at(
        "(handler-bind ((error (lambda (condition) (recover condition outer)))) (use condition outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "condition"]);
}

#[test]
fn treats_restart_bind_restart_option_lambda_lists_as_local() {
    let params = infer_at(
        "(restart-bind ((retry (lambda () stream) :report (lambda (stream) stream) :test test-fn)) (invoke stream outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["stream", "test-fn", "outer"]);
}

#[test]
fn treats_dolist_iteration_variable_as_local_to_result_and_body() {
    let params = infer_at(
        "(dolist (item items (finish item done)) (render item outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["items", "done", "outer"]);
}

#[test]
fn treats_dotimes_iteration_variable_as_local_to_result_and_body() {
    let params = infer_at(
        "(dotimes (index count (finish index done)) (render index outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["count", "done", "outer"]);
}

#[test]
fn treats_do_variables_as_local_to_steps_end_clause_and_body() {
    let params = infer_at(
        "(do ((i start (1+ i)) (sum 0 (+ sum i))) ((>= i limit) (finish sum done)) (render i sum outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["start", "limit", "done", "outer"]);
}

#[test]
fn treats_do_star_variables_as_local_to_later_inits_steps_and_body() {
    let params = infer_at(
        "(do* ((i start (1+ i)) (sum i (+ sum i))) ((>= sum limit) done) (render sum outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["start", "limit", "done", "outer"]);
}

#[test]
fn treats_prog_variables_as_local_to_body_but_not_parallel_inits() {
    let params = infer_at(
        "(prog ((value seed) (copy value)) (setf value copy) (return (finish value outer)))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["seed", "value", "outer"]);
}

#[test]
fn treats_prog_star_variables_as_local_to_later_inits_and_body() {
    let params = infer_at(
        "(prog* ((value seed) (copy value)) (setf value copy) (return outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["seed", "outer"]);
}

#[test]
fn treats_with_slots_names_as_local_to_body() {
    let params = infer_at(
        "(with-slots (width (height slot-height)) panel (list width height outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["panel", "outer"]);
}

#[test]
fn treats_with_accessors_names_as_local_to_body() {
    let params = infer_at(
        "(with-accessors ((width panel-width) height) panel (list width height outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["panel", "outer"]);
}

#[test]
fn skips_common_lisp_declaration_forms_in_body_scans() {
    let params = infer_at(
        "(locally (declare (special target)) (+ target outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["target", "outer"]);
}

#[test]
fn treats_flet_lambda_list_as_local_to_function_body() {
    let params = infer_at(
        "(flet ((helper (local) (+ local outer))) (helper input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_labels_recursive_callable_names_as_non_params() {
    let params = infer_at(
        "(labels ((helper (local) (if local (helper outer) outer))) (helper input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_macrolet_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(macrolet ((with-local (local) (list local outer))) (with-local input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_cl_user_macrolet_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(cl-user:macrolet ((with-local (local) (list local outer))) (with-local input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_common_lisp_macrolet_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(cl:macrolet ((with-local (local) (list local outer))) (with-local input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_compiler_macrolet_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(compiler-macrolet ((with-local (local) (list local outer))) (with-local input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_cl_user_compiler_macrolet_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(cl-user:compiler-macrolet ((with-local (local) (list local outer))) (with-local input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_common_lisp_compiler_macrolet_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(cl:compiler-macrolet ((with-local (local) (list local outer))) (with-local input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn treats_emacs_lisp_let_bindings_as_local_to_body() {
    let params = infer_at_dialect(
        Dialect::EmacsLisp,
        "(let ((local input)) (+ local outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["input", "outer"]);
}

#[test]
fn treats_emacs_lisp_symbol_macrolet_bindings_as_local_to_body() {
    let params = infer_at_dialect(
        Dialect::EmacsLisp,
        "(cl-symbol-macrolet ((local (compute outer))) (list local outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn treats_emacs_lisp_flet_lambda_list_as_local_to_function_body() {
    let params = infer_at_dialect(
        Dialect::EmacsLisp,
        "(cl-flet ((helper (local) (+ local outer))) (helper input))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "input"]);
}

#[test]
fn resolver_binding_scopes_preserve_dialect_visibility() {
    let cases = [
        (
            Dialect::EmacsLisp,
            "(let* ((first seed) (second first)) (list first second outer))",
            &["seed", "outer"][..],
        ),
        (
            Dialect::Scheme,
            "(let ((first seed) (second first)) (list first second outer))",
            &["seed", "first", "outer"][..],
        ),
        (
            Dialect::Clojure,
            "(let [first seed second first] [first second outer])",
            &["seed", "outer"][..],
        ),
        (
            Dialect::Janet,
            "(let [first seed second first] [first second outer])",
            &["seed", "outer"][..],
        ),
        (
            Dialect::Fennel,
            "(let [first seed second first] [first second outer])",
            &["seed", "outer"][..],
        ),
    ];

    for (dialect, input, expected) in cases {
        assert_eq!(
            infer_at_dialect(dialect, input, &[0], &[]),
            expected,
            "{dialect:?}"
        );
    }
}

#[test]
fn verified_dialects_exclude_callable_parameters() {
    let cases = [
        (Dialect::CommonLisp, "(lambda (local) (list local outer))"),
        (Dialect::EmacsLisp, "(lambda (local) (list local outer))"),
        (Dialect::Scheme, "(lambda (local) (list local outer))"),
        (Dialect::Clojure, "(fn [local] [local outer])"),
        (Dialect::Janet, "(fn [local] [local outer])"),
        (Dialect::Fennel, "(fn [local] [local outer])"),
    ];

    for (dialect, input) in cases {
        assert_eq!(
            infer_at_dialect(dialect, input, &[0], &[]),
            ["outer"],
            "{dialect:?}"
        );
    }
}

#[test]
fn verified_dialects_exclude_definition_parameters() {
    let cases = [
        (
            Dialect::CommonLisp,
            "(defun render (local) (list local outer))",
        ),
        (
            Dialect::EmacsLisp,
            "(defun render (local) (list local outer))",
        ),
        (
            Dialect::Scheme,
            "(define (render local) (list local outer))",
        ),
        (Dialect::Clojure, "(defn render [local] [local outer])"),
        (Dialect::Janet, "(defn render [local] [local outer])"),
        (Dialect::Fennel, "(fn render [local] [local outer])"),
    ];

    for (dialect, input) in cases {
        assert_eq!(
            infer_at_dialect(dialect, input, &[0], &[]),
            ["outer"],
            "{dialect:?}"
        );
    }
}

#[test]
fn treats_scheme_named_let_name_and_parameters_as_local() {
    let params = infer_at_dialect(
        Dialect::Scheme,
        "(let loop ((local seed)) (list loop local outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["seed", "outer"]);
}

#[test]
fn treats_clojure_named_multi_arity_fn_bindings_as_local() {
    let params = infer_at_dialect(
        Dialect::Clojure,
        "(fn render ([local] [render local outer]) ([local other] [render local other extra]))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer", "extra"]);
}

#[test]
fn treats_define_setf_expander_macro_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(define-setf-expander slot (&whole whole &environment env target) (list whole env target outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn treats_define_compiler_macro_lambda_list_as_local_to_expander_body() {
    let params = infer_at(
        "(define-compiler-macro render (&whole whole &environment env target) (list whole env target outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn treats_package_qualified_compiler_macro_lambda_list_bindings_as_local() {
    let params = infer_at(
        "(define-compiler-macro render (&whole whole &environment env cl:target) (list whole env target outer))",
        &[0],
        &[],
    );

    assert_eq!(params, vec!["outer"]);
}

#[test]
fn keeps_flet_callable_name_as_free_value_reference() {
    let params = infer_at("(flet ((helper () outer)) (list helper))", &[0], &[]);

    assert_eq!(params, vec!["outer", "helper"]);
}

#[test]
fn excludes_destructured_lambda_parameters_and_explicit_params() {
    let params = infer_at_dialect(
        Dialect::CommonLisp,
        "(lambda ((inner)) (+ inner outer ignored))",
        &[0],
        &["ignored"],
    );

    assert_eq!(params, vec!["outer"]);
}
