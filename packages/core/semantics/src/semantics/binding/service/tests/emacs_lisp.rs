//! The binding table for Emacs Lisp.
//!
//! Fixtures carry a `lexical-binding: t` header unless the test is about its
//! absence, because without it every `let` in the file binds dynamically and
//! the answers below would all change — which is itself one of the tests.

use paredit_core_syntax::dialect::Dialect;

use super::super::super::model::{BindingKind, BindingTable, SpecialBinding};
use super::{binding_at, binding_labels, build_in, reference_labels};

/// A file header that turns lexical binding on, so a plain `let` binds
/// lexically.
const LEXICAL: &str = ";;; -*- lexical-binding: t -*-\n";

fn build(body: &str) -> (BindingTable, String) {
    let input = format!("{LEXICAL}{body}");
    (build_in(Dialect::EmacsLisp, &input), input)
}

fn names(body: &str) -> Vec<String> {
    let (table, input) = build(body);
    binding_labels(&table, &input)
        .into_iter()
        .map(|label| label.split('@').next().unwrap_or_default().to_owned())
        .collect()
}

/// The text of every reference attributed to the binding whose defining atom
/// is the first occurrence of `needle` in the body.
fn references_to(body: &str, needle: &str) -> Vec<String> {
    let (table, input) = build(body);
    let offset = input.find(needle).expect("needle is in the fixture");
    let id = binding_at(&table, offset);
    reference_labels(&table, id, &input)
        .into_iter()
        .map(|label| label.split('@').next().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_plain_let_binds_and_its_body_reference_resolves() {
    assert_eq!(names("(let ((x 1)) x)"), ["x"]);
    assert_eq!(references_to("(let ((x 1)) x)", "x 1"), ["x"]);
}

#[test]
fn a_parallel_let_initializer_sees_the_outer_binding() {
    // `(let ((x 1)) (let ((x x)) …))`: the inner initializer reads the
    // *outer* `x`, because a parallel `let` binds nothing until its body.
    let body = "(let ((outer 1)) (let ((outer outer)) outer))";
    assert_eq!(references_to(body, "outer 1"), ["outer"]);
    assert_eq!(references_to(body, "outer outer"), ["outer"]);
}

#[test]
fn let_star_makes_each_name_visible_to_the_next_initializer() {
    let body = "(let* ((a 1) (b a)) b)";
    assert_eq!(references_to(body, "a 1"), ["a"]);
    assert_eq!(references_to(body, "b a"), ["b"]);
}

#[test]
fn letrec_makes_every_name_visible_to_every_initializer() {
    // The whole point of `letrec`: a closure built in the first initializer
    // can call one bound by the second.
    let body = "(letrec ((even (lambda (n) (funcall odd n)))\n         (odd (lambda (n) (funcall even n))))\n  even)";
    assert_eq!(references_to(body, "odd (lambda"), ["odd"]);
    assert_eq!(references_to(body, "even (lambda"), ["even", "even"]);
}

#[test]
fn the_subr_x_conditional_binders_are_walked() {
    for body in [
        "(when-let* ((v (compute))) (use v))",
        "(if-let* ((v (compute))) (use v))",
        "(and-let* ((v (compute))) (use v))",
        "(while-let ((v (compute))) (use v))",
    ] {
        assert_eq!(names(body), ["v"], "{body}");
        assert_eq!(references_to(body, "v (compute)"), ["v"], "{body}");
    }
}

#[test]
fn an_if_let_else_branch_does_not_see_the_bindings() {
    // `(if-let* (BINDINGS) THEN ELSE…)` runs ELSE precisely when a binding's
    // value was nil, so nothing is in scope there. A walk that treated the
    // whole tail as one body would attribute the ELSE occurrence of `v` to
    // the binding, and a rename would then rewrite an unrelated global.
    let body = "(if-let* ((v (compute))) (use v) (fallback v))";
    assert_eq!(references_to(body, "v (compute)"), ["v"]);
}

#[test]
fn pcase_let_binds_the_names_inside_a_backquoted_pattern() {
    let body = "(pcase-let ((`(,head . ,tail) (compute))) (list head tail))";
    let (table, input) = build(body);

    let bound: Vec<_> = binding_labels(&table, &input)
        .into_iter()
        .map(|label| label.split('@').next().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(bound, ["head", "tail"]);
}

#[test]
fn a_pcase_pattern_operator_is_not_read_as_a_binding() {
    // `pred` and `guard` are pattern operators; only `n` is bound.
    let body = "(pcase-let ((`(,n) (compute))) n)";
    assert_eq!(names(body), ["n"]);
    assert!(!names("(pcase-let (((pred integerp) (compute))) t)").contains(&"pred".to_owned()));
}

#[test]
fn seq_let_binds_a_destructuring_pattern() {
    let body = "(seq-let [first second] (compute) (list first second))";
    assert_eq!(names(body), ["first", "second"]);
}

#[test]
fn cl_destructuring_bind_binds_its_arglist() {
    let body = "(cl-destructuring-bind (a &optional b) (compute) (list a b))";
    assert_eq!(names(body), ["a", "b"]);
    assert_eq!(references_to(body, "a &optional"), ["a"]);
}

#[test]
fn dolist_binds_the_loop_variable_over_the_body_and_the_result_form() {
    // The result form runs after the loop with the variable still bound,
    // which is where `dolist` idiomatically reverses an accumulator.
    let body = "(dolist (item items (finish item)) (use item))";
    assert_eq!(names(body), ["item"]);
    assert_eq!(references_to(body, "item items"), ["item", "item"]);
}

#[test]
fn pcase_dolist_binds_the_pattern_names() {
    let body = "(pcase-dolist (`(,key . ,value) alist) (use key value))";
    assert_eq!(names(body), ["key", "value"]);
}

#[test]
fn cl_labels_lets_a_local_function_call_itself_and_its_siblings() {
    let body = "(cl-labels ((f (n) (g n)) (g (n) (f n))) (f 1))";
    assert_eq!(names(body), ["f", "g", "n", "n"]);
    // `f` is called from `g`'s body and from the `cl-labels` body.
    assert_eq!(references_to(body, "f (n)"), ["f", "f"]);
}

#[test]
fn cl_flet_definitions_do_not_see_each_other() {
    // `cl-flet` closes each definition over the *enclosing* scope, so the
    // `g` inside `f`'s body is not the sibling.
    let body = "(cl-flet ((f (n) (g n)) (g (n) n)) (f 1))";
    assert_eq!(references_to(body, "g (n) n"), [] as [String; 0]);
}

#[test]
fn a_function_designator_reaches_a_local_function_here_too() {
    // Emacs Lisp is the other two-namespace dialect, and `#'` means the same
    // thing in it. `#'f` is how a `cl-flet` is handed to `mapcar`.
    let body = "(cl-flet ((f (n) n)) (mapcar #'f list))";
    assert_eq!(references_to(body, "f (n) n"), ["f"]);

    let long_hand = "(cl-flet ((f (n) n)) (mapcar (function f) list))";
    assert_eq!(references_to(long_hand, "f (n) n"), ["f"]);

    // And still never a variable: `#'f` under `(let ((f 1)) ...)` names the
    // global function, not the binding.
    let variable = "(let ((f 1)) (mapcar #'f list))";
    assert_eq!(references_to(variable, "f 1"), [] as [String; 0]);
}

#[test]
fn cl_flet_star_extends_the_group_one_definition_at_a_time() {
    let body = "(cl-flet* ((f (n) n) (g (n) (f n))) (g 1))";
    // `f` is visible inside `g` and in the body, but not the other way round.
    assert_eq!(references_to(body, "f (n) n"), ["f"]);
}

#[test]
fn a_named_let_binds_its_loop_name_as_a_function() {
    let body = "(named-let walk ((n 10)) (if (zerop n) n (walk (1- n))))";
    let (table, input) = build(body);

    let offset = input.find("walk ((n").expect("loop name");
    let id = binding_at(&table, offset);
    assert_eq!(table.binding(id).kind(), BindingKind::Function);
    assert_eq!(reference_labels(&table, id, &input).len(), 1);
}

#[test]
fn condition_case_binds_its_variable_in_the_handlers_and_not_the_body() {
    // This form reads backwards: `err` is *not* in scope in the protected
    // form, only in the handlers.
    let body = "(condition-case err (risky err) (error (report err)))";
    assert_eq!(references_to(body, "err (risky"), ["err"]);
}

#[test]
fn condition_case_with_nil_binds_nothing() {
    assert_eq!(
        names("(condition-case nil (risky) (error nil))"),
        [] as [String; 0]
    );
}

#[test]
fn a_lambda_parameter_list_opens_a_scope() {
    let body = "(lambda (a &optional b &rest rest) (list a b rest))";
    assert_eq!(names(body), ["a", "b", "rest"]);
}

#[test]
fn a_defun_binds_its_parameters_but_not_its_own_name() {
    // The name is a global; this table is the lexical context of one file.
    let body = "(defun my-fn (a b) (+ a b))";
    assert_eq!(names(body), ["a", "b"]);
}

#[test]
fn a_leading_declare_form_is_not_walked_as_a_call() {
    // `(declare (indent 1))` names no function `indent`. Walking into it
    // used to mark every enclosing binding opaque on a head that is not one.
    let body = "(let ((x 1)) (defun f () (declare (indent 1)) x))";
    let (table, input) = build(body);
    let id = binding_at(&table, input.find("x 1").expect("binding"));
    assert!(table.binding(id).opacity().is_transparent());
}

#[test]
fn cl_defmethod_finds_its_arglist_past_a_qualifier() {
    let body = "(cl-defmethod handle :around ((obj my-type) arg) (list obj arg))";
    assert_eq!(names(body), ["obj", "arg"]);
}

#[test]
fn a_defvar_name_makes_every_let_binding_of_it_dynamic() {
    // Under `lexical-binding: t` a plain `let` is lexical — unless the name
    // was declared with `defvar`, in which case the same source text binds
    // dynamically and every callee can read it.
    let body = "(defvar my-special nil)\n(let ((my-special 1) (ordinary 2)) (run))";
    let (table, input) = build(body);

    let special = binding_at(&table, input.find("my-special 1").expect("binding"));
    let lexical = binding_at(&table, input.find("ordinary 2").expect("binding"));
    assert_eq!(
        table.binding(special).special(),
        SpecialBinding::DeclaredSpecial
    );
    assert_eq!(table.binding(lexical).special(), SpecialBinding::Lexical);
}

#[test]
fn without_the_header_every_variable_binding_in_the_file_is_dynamic() {
    // No `lexical-binding: t`, so Emacs reads the whole file under dynamic
    // binding and `x` is readable by anything `run` calls.
    let input = "(let ((x 1)) (run))";
    let table = build_in(Dialect::EmacsLisp, input);
    let id = binding_at(&table, input.find('x').expect("binding"));

    assert_eq!(
        table.binding(id).special(),
        SpecialBinding::DeclaredSpecial,
        "a file without the header binds dynamically throughout"
    );
}

#[test]
fn dlet_binds_dynamically_even_in_a_lexical_file() {
    let body = "(dlet ((x 1)) (run))";
    let (table, input) = build(body);
    let id = binding_at(&table, input.find("x 1").expect("binding"));

    assert_eq!(table.binding(id).special(), SpecialBinding::DeclaredSpecial);
}

#[test]
fn setq_is_recorded_as_an_assignment_against_the_visible_binding() {
    let body = "(let ((x 1)) (setq x 2) x)";
    let (table, input) = build(body);
    let id = binding_at(&table, input.find("x 1").expect("binding"));

    assert_eq!(table.binding(id).assignments().len(), 1);
}

#[test]
fn a_reference_is_matched_case_sensitively() {
    // The Emacs Lisp reader does not fold case, so `X` in the body is a free
    // global and not a use of the binding.
    let body = "(let ((x 1)) X)";
    assert_eq!(references_to(body, "x 1"), [] as [String; 0]);
}

#[test]
fn a_known_call_does_not_make_the_enclosing_scope_opaque() {
    let body = "(let ((x 1)) (message \"%s\" (length x)))";
    let (table, input) = build(body);
    let id = binding_at(&table, input.find("x 1").expect("binding"));

    assert!(table.binding(id).opacity().is_transparent());
}

#[test]
fn an_unknown_head_still_makes_the_enclosing_scope_opaque() {
    // It could be a macro expanding into `(setq x …)`, and nothing in the
    // source rules that out.
    let body = "(let ((x 1)) (my-package-macro x))";
    let (table, input) = build(body);
    let id = binding_at(&table, input.find("x 1").expect("binding"));

    assert!(!table.binding(id).opacity().is_transparent());
}

#[test]
fn quoted_data_holds_no_references() {
    let body = "(let ((x 1)) '(x x) x)";
    assert_eq!(references_to(body, "x 1"), ["x"]);
}

#[test]
fn an_unanalysed_dialect_still_gets_an_empty_table() {
    // Four dialects are walked — Common Lisp, Emacs Lisp, Scheme, Racket. The
    // rest have binding forms different enough that a shared traversal would
    // have to guess, so this layer records nothing for them rather than a
    // guess.
    for dialect in [Dialect::Lfe, Dialect::Hy, Dialect::Unknown] {
        let table = build_in(dialect, "(let ((x 1)) x)");
        assert_eq!(table.bindings().count(), 0, "{dialect:?}");
    }
}
