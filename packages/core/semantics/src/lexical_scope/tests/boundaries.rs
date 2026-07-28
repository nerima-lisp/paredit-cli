use super::*;

#[test]
fn define_setf_expander_body_is_definition_scope_boundary() {
    let input = "(list outer (define-setf-expander slot (place) (list outer place)) outer)";

    assert_eq!(reference_texts(input, "outer"), vec!["outer", "outer"]);
}

#[test]
fn define_compiler_macro_body_is_definition_scope_boundary() {
    let input = "(list outer (define-compiler-macro render (place) (list outer place)) outer)";

    assert_eq!(reference_texts(input, "outer"), vec!["outer", "outer"]);
}

#[test]
fn defun_body_references_are_scanned() {
    let input = "(list used (defun caller () (used)) used)";

    assert_eq!(reference_texts(input, "used"), vec!["used", "used", "used"]);
}

#[test]
fn declare_forms_in_common_lisp_bodies_are_not_counted_as_references() {
    let input = "(list used (locally (declare (special used)) used) used)";

    assert_eq!(reference_texts(input, "used"), vec!["used", "used", "used"]);
}

#[test]
fn function_type_specifier_is_not_treated_as_an_opaque_function_designator() {
    // `(function name)` is the explicit spelling of `#'name` and is opaque
    // (see `reader_prefixed_quote_and_function_still_block_references`
    // above). `(function (arg-types...) return-type)` shares the same head
    // but is the unrelated FUNCTION *type specifier*, most commonly seen
    // inside `declaim`/`ftype` for a `deftype`-defined alias — its contents
    // are ordinary type-position atoms and must still be scanned.
    let input = "(declaim (ftype (function (my-word) my-word) f))";

    assert_eq!(
        reference_texts(input, "my-word"),
        vec!["my-word", "my-word"]
    );
}

#[test]
fn a_quote_wrapping_an_unquote_inside_quasiquote_is_still_a_live_reference() {
    // `',x` — a quote wrapping an unquote — is the standard idiom for
    // splicing a computed value as a literal into a macro's generated
    // code, e.g. `` `(setf (get ',name 'prop) ',computed)) ``. The quote
    // does not make this opaque: it is only reachable while already inside
    // an active quasiquote template, so it must keep descending to find
    // the nested unquote's reference. A top-level (`quasiquote_depth ==
    // 0`) plain quote is unaffected and stays fully opaque, since `,`/`,@`
    // have no meaning there and it can never contain a live reference.
    let input = "(list x `(setf (get 'y 'prop) ',x))";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x"]);
}

#[test]
fn proclamation_forms_in_common_lisp_bodies_are_not_counted_as_references() {
    let input =
        "(list used (locally (declaim (special used)) (proclaim (special used)) used) used)";

    assert_eq!(reference_texts(input, "used"), vec!["used", "used", "used"]);
}

#[test]
fn quote_form_is_reference_boundary() {
    let input = "(list x (quote x) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x"]);
}

#[test]
fn function_form_is_reference_boundary() {
    let input = "(list fn (function fn) fn)";

    assert_eq!(reference_texts(input, "fn"), vec!["fn", "fn"]);
}

#[test]
fn reader_prefixed_function_lambda_bodies_are_still_scanned() {
    let input = "(list y #'(lambda (x) y) y)";

    assert_eq!(reference_texts(input, "y"), vec!["y", "y", "y"]);
}

#[test]
fn reader_prefixed_non_lambda_lists_remain_boundaries() {
    let input = "(list y #'(foo y) y)";

    assert_eq!(reference_texts(input, "y"), vec!["y", "y"]);
}

#[test]
fn quasiquote_form_counts_unquoted_references_only() {
    let input = "(list x (quasiquote (x (unquote x))) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x"]);
}

#[test]
fn reader_prefixed_quote_and_function_still_block_references() {
    let input = "(list x 'x #'x `(hold ,x ,@rest) x rest)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x"]);
    assert_eq!(reference_texts(input, "rest"), vec!["rest", "rest"]);
}

#[test]
fn nested_quasiquote_requires_matching_unquote_depth() {
    let input = "(list x `(outer `(inner ,x ,,x)) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x"]);
}
