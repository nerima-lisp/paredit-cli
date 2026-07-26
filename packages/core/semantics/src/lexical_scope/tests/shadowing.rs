use super::*;

#[test]
fn skips_shadowed_lambda_parameter_references() {
    let input = "(list x (lambda (x) x))";

    assert_eq!(reference_texts(input, "x"), vec!["x"]);
}

#[test]
fn sequential_let_stops_after_shadowing_binding() {
    let input = "(let* ((y x) (x 2)) (list x y))";

    assert_eq!(reference_texts(input, "x"), vec!["x"]);
}

#[test]
fn parallel_let_checks_binding_values_before_body_shadowing() {
    let input = "(let ((x 1) (y x)) (list x y))";

    assert_eq!(reference_texts(input, "x"), vec!["x"]);
}

#[test]
fn bare_uninitialized_let_binding_still_scans_the_body() {
    // `(let (rows) ...)` is CLHS-legal shorthand for `(let ((rows nil)) ...)`.
    // A bare binding name (no value form) must not stop the body scan for an
    // unrelated symbol referenced later — this previously made a live call
    // invisible, so `remove-unused-definitions` would delete the callee.
    let input = "(let (rows) (helper rows))";

    assert_eq!(reference_texts(input, "helper"), vec!["helper"]);
}

#[test]
fn bare_uninitialized_sequential_let_binding_still_scans_the_body() {
    let input = "(let* (rows) (helper rows))";

    assert_eq!(reference_texts(input, "helper"), vec!["helper"]);
}

#[test]
fn symbol_macrolet_checks_expansions_before_body_shadowing() {
    let input = "(symbol-macrolet ((x outer) (y x)) (list x y outer))";

    assert_eq!(reference_texts(input, "x"), vec!["x"]);
    assert_eq!(reference_texts(input, "outer"), vec!["outer", "outer"]);
}

#[test]
fn package_qualified_lexical_bindings_shadow_unqualified_references() {
    let input = "(list x (let ((cl-user:x x) (y cl-user:x)) (list x y)) x)";

    assert_eq!(
        reference_texts(input, "x"),
        vec!["x", "x", "cl-user:x", "x"]
    );
}

#[test]
fn package_qualified_sequential_bindings_shadow_later_initializers_and_body() {
    let input = "(list x (let* ((cl-user:x x) (y x)) (list x y)) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x"]);
}

#[test]
fn clojure_vector_let_is_sequential_for_shadowing() {
    let input = "(let [y x x 2] (list x y))";

    assert_eq!(reference_texts(input, "x"), vec!["x"]);
}

#[test]
fn clojure_destructuring_shadows_keys_shorthand() {
    let input = "(list x (fn [{:keys [x] :as m}] x m))";

    assert_eq!(reference_texts(input, "x"), vec!["x"]);
}

#[test]
fn lambda_list_default_forms_remain_outer_references() {
    let input = "(list fallback (lambda (&optional (x (fallback y) supplied)) x))";

    assert_eq!(
        reference_texts(input, "fallback"),
        vec!["fallback", "fallback"]
    );
}

#[test]
fn lambda_list_default_forms_count_same_name_outer_references_before_shadowing() {
    let input = "(list x (lambda (&optional (x x)) x))";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x"]);
}

#[test]
fn defun_lambda_list_default_forms_remain_outer_references() {
    // DEFUN's own branch used to scan only its body, skipping the parameter
    // list entirely: a global referenced solely as an &optional/&key/&aux
    // default value (`(defun f (&optional (y *default*)) ...)`) had zero
    // recorded references, so unused-definition-report/remove-unused-
    // definitions would report and delete a still-live defparameter.
    let input = "(list fallback (defun f (&optional (x (fallback y) supplied)) x))";

    assert_eq!(
        reference_texts(input, "fallback"),
        vec!["fallback", "fallback"]
    );
}

#[test]
fn defun_parameter_shadows_same_named_global_in_body() {
    let input = "(list x (defun f (x) (list x)) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x"]);
}

#[test]
fn labels_definition_bodies_do_not_see_their_own_binding_name() {
    let input = "(list m (labels ((m (x) (list m x))) (m m)) m)";

    assert_eq!(reference_texts(input, "m"), vec!["m", "m"]);
}

#[test]
fn package_qualified_labels_names_shadow_unqualified_references() {
    let input = "(list m (labels ((cl-user:m (x) (list m x))) (m m)) m)";

    assert_eq!(reference_texts(input, "m"), vec!["m", "m"]);
}

#[test]
fn macrolet_expander_bodies_remain_outer_references() {
    let input = "(list m (macrolet ((m (x) (list m x))) (m m)) m)";

    assert_eq!(reference_texts(input, "m"), vec!["m", "m", "m"]);
}

#[test]
fn package_qualified_macrolet_expander_bodies_remain_outer_references() {
    let input = "(list m (cl:macrolet ((cl-user:m (x) (list m x))) (m m)) m)";

    assert_eq!(reference_texts(input, "m"), vec!["m", "m", "m"]);
}

#[test]
fn compiler_macrolet_expander_bodies_remain_outer_references() {
    let input = "(list m (compiler-macrolet ((m (x) (list m x))) (m m)) m)";

    assert_eq!(reference_texts(input, "m"), vec!["m", "m", "m"]);
}

#[test]
fn package_qualified_compiler_macrolet_expander_bodies_remain_outer_references() {
    let input = "(list m (cl-user:compiler-macrolet ((cl:m (x) (list m x))) (m m)) m)";

    assert_eq!(reference_texts(input, "m"), vec!["m", "m", "m"]);
}

#[test]
fn common_lisp_lambda_parameters_preserve_shadowing_contract() {
    assert!(
        reference_texts_with_dialect("(lambda (value) value)", Dialect::CommonLisp, "value",)
            .is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(lambda (other) value)", Dialect::CommonLisp, "value",),
        ["value"]
    );
}

#[test]
fn emacs_lisp_lambda_parameters_preserve_shadowing_contract() {
    assert!(
        reference_texts_with_dialect("(lambda (value) value)", Dialect::EmacsLisp, "value",)
            .is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(lambda (other) value)", Dialect::EmacsLisp, "value",),
        ["value"]
    );
}

#[test]
fn scheme_variadic_lambda_atom_shadows_its_body() {
    assert!(
        reference_texts_with_dialect("(lambda value value)", Dialect::Scheme, "value",).is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(lambda other value)", Dialect::Scheme, "value"),
        ["value"]
    );
}

#[test]
fn clojure_anonymous_fn_parameters_shadow_their_body() {
    assert!(
        reference_texts_with_dialect("(fn [value] value)", Dialect::Clojure, "value",).is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(fn [other] value)", Dialect::Clojure, "value"),
        ["value"]
    );
}

#[test]
fn clojure_named_fn_name_and_parameters_shadow_their_body() {
    assert!(
        reference_texts_with_dialect(
            "(fn recur-name [value] (list recur-name value))",
            Dialect::Clojure,
            "recur-name",
        )
        .is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(fn recur-name [other] value)", Dialect::Clojure, "value",),
        ["value"]
    );
}

#[test]
fn clojure_multi_arity_fn_scopes_each_parameter_clause() {
    assert_eq!(
        reference_texts_with_dialect(
            "(fn ([value] value) ([other] value))",
            Dialect::Clojure,
            "value",
        ),
        ["value"]
    );
    assert!(
        reference_texts_with_dialect(
            "(fn recur-name ([value] recur-name) ([other] recur-name))",
            Dialect::Clojure,
            "recur-name",
        )
        .is_empty()
    );
}

#[test]
fn janet_fn_vector_parameters_shadow_their_body() {
    assert!(
        reference_texts_with_dialect("(fn [value] value)", Dialect::Janet, "value",).is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(fn [other] value)", Dialect::Janet, "value"),
        ["value"]
    );
}

#[test]
fn fennel_fn_vector_parameters_shadow_their_body() {
    assert!(
        reference_texts_with_dialect("(fn [value] value)", Dialect::Fennel, "value",).is_empty()
    );
    assert_eq!(
        reference_texts_with_dialect("(fn [other] value)", Dialect::Fennel, "value"),
        ["value"]
    );
}
