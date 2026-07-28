//! What each binder makes visible, and where.

use super::{binding_at, binding_labels, build, reference_labels};
use crate::semantics::binding::model::{BindingKind, ScopeOpacity, SpecialBinding};

#[test]
fn a_parallel_let_hides_its_own_names_from_its_initial_values() {
    // The `a` at 15 is the *outer* `a`, so nothing in this form binds it and
    // the table leaves it unresolved rather than attributing it to `a@7`.
    let input = "(let ((a 1) (b a)) b)";

    let table = build(input);
    assert_eq!(binding_labels(&table, input), vec!["a@7", "b@13"]);
    assert!(reference_labels(&table, binding_at(&table, 7), input).is_empty());
    assert_eq!(
        reference_labels(&table, binding_at(&table, 13), input),
        vec!["b@19"]
    );
}

#[test]
fn a_sequential_let_shows_each_name_to_the_next_initial_value() {
    let input = "(let* ((a 1) (b a)) b)";

    let table = build(input);
    assert_eq!(
        reference_labels(&table, binding_at(&table, 8), input),
        vec!["a@16"]
    );
    assert_eq!(
        reference_labels(&table, binding_at(&table, 14), input),
        vec!["b@20"]
    );
}

#[test]
fn a_sequential_let_still_reads_the_outer_name_in_its_own_initial_value() {
    // `(let* ((x x)) ...)`: the right-hand `x` is bound by the *outer* let,
    // because a binding is not visible until its own value form is done.
    let input = "(let ((x 1)) (let* ((x x)) x))";

    let table = build(input);
    let outer = binding_at(&table, 7);
    let inner = binding_at(&table, 21);
    assert_eq!(reference_labels(&table, outer, input), vec!["x@23"]);
    assert_eq!(reference_labels(&table, inner, input), vec!["x@27"]);
}

#[test]
fn the_innermost_binding_of_a_name_owns_the_reference() {
    let input = "(let ((x 1)) (let ((x 2)) x))";

    let table = build(input);
    assert!(reference_labels(&table, binding_at(&table, 7), input).is_empty());
    assert_eq!(
        reference_labels(&table, binding_at(&table, 20), input),
        vec!["x@26"]
    );
}

#[test]
fn an_unknown_macro_head_makes_the_enclosing_scope_opaque() {
    let input = "(let ((x 1)) (my-macro x))";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert_eq!(
        table.binding(x).opacity(),
        ScopeOpacity::ContainsOpaqueRegion
    );
    assert!(!table.binding(x).is_propagatable());
    // The walk still descends: opacity is a warning to the value layer, not a
    // reason to lose the reference.
    assert_eq!(reference_labels(&table, x, input), vec!["x@23"]);
}

#[test]
fn a_standard_function_call_stays_transparent() {
    // A function receives its arguments' values and cannot reach the caller's
    // lexical environment, so `+` provably cannot touch `x`. Treating it as
    // opaque would also be sound, and would leave the value layer unable to
    // say anything about any realistic file.
    let input = "(let ((x 1)) (+ x 1))";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert_eq!(table.binding(x).opacity(), ScopeOpacity::Transparent);
    assert!(table.binding(x).is_propagatable());
}

#[test]
fn an_unknown_head_is_opaque_because_it_might_be_a_macro() {
    // `my-macro` could expand into `(setq x 2)`, leaving nothing in the source
    // for the assignment collector to find.
    let input = "(let ((x 1)) (my-macro x))";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert_eq!(
        table.binding(x).opacity(),
        ScopeOpacity::ContainsOpaqueRegion
    );
    assert!(!table.binding(x).is_propagatable());
}

#[test]
fn a_scope_with_no_calls_at_all_stays_transparent() {
    let input = "(let ((x 1)) x)";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert_eq!(table.binding(x).opacity(), ScopeOpacity::Transparent);
    assert!(table.binding(x).is_propagatable());
}

#[test]
fn quoted_data_holds_no_references_but_leaves_the_scope_readable() {
    // Two claims that used to be one. The walk stops at `'(x)` because the
    // `x` in there is a symbol in a list, not a use of the binding — that
    // half is unchanged. What it is *not* is a reason to distrust the scope:
    // quoted data is never evaluated, so it cannot reassign anything.
    let input = "(let ((x 1)) '(x))";

    let table = build(input);
    let x = binding_at(&table, 7);
    assert!(reference_labels(&table, x, input).is_empty());
    assert_eq!(table.binding(x).opacity(), ScopeOpacity::Transparent);
}

#[test]
fn an_unquote_inside_a_quasiquote_is_a_live_reference() {
    let input = "(let ((x 1)) `(a ,x))";

    let table = build(input);
    assert_eq!(
        reference_labels(&table, binding_at(&table, 7), input),
        vec!["x@18"]
    );
}

#[test]
fn a_function_and_a_variable_of_the_same_name_are_separate_bindings() {
    // Head position reads the function namespace, every other position the
    // value namespace, so each reference lands on its own binding.
    let input = "(flet ((f () 1)) (let ((f 2)) (list f (f))))";

    let table = build(input);
    let function = binding_at(&table, 8);
    let variable = binding_at(&table, 24);
    assert_eq!(table.binding(function).kind(), BindingKind::Function);
    assert_eq!(table.binding(variable).kind(), BindingKind::Variable);
    assert_eq!(reference_labels(&table, variable, input), vec!["f@36"]);
    assert_eq!(reference_labels(&table, function, input), vec!["f@39"]);
}

#[test]
fn a_bare_name_never_reads_a_local_function() {
    // The spec case: `(flet ((x ...)) x)` reads a *variable* `x`, which is
    // free here, so the table records nothing rather than the local function.
    let input = "(flet ((x () 1)) x)";

    let table = build(input);
    assert!(reference_labels(&table, binding_at(&table, 8), input).is_empty());
}

#[test]
fn labels_sees_itself_but_flet_does_not() {
    let recursive = "(labels ((f (n) (f n))) (f 1))";
    let table = build(recursive);
    assert_eq!(
        reference_labels(&table, binding_at(&table, 10), recursive),
        vec!["f@17", "f@25"]
    );

    let shadowed = "(flet ((f (n) (f n))) (f 1))";
    let table = build(shadowed);
    // The `f@15` inside the definition is the *outer* `f`: an `flet` body
    // cannot call itself, so only the `flet` body's `f@23` is attributed.
    assert_eq!(
        reference_labels(&table, binding_at(&table, 8), shadowed),
        vec!["f@23"]
    );
}

#[test]
fn a_lambda_list_default_reads_the_parameters_to_its_left_only() {
    let input = "(lambda (a &optional (b a)) (list a b))";

    let table = build(input);
    assert_eq!(
        reference_labels(&table, binding_at(&table, 9), input),
        vec!["a@24", "a@34"]
    );
    assert_eq!(
        reference_labels(&table, binding_at(&table, 22), input),
        vec!["b@36"]
    );
}

#[test]
fn a_declared_special_binding_is_marked_and_never_propagates() {
    let input = "(defvar *x*)\n(let ((*x* 1)) *x*)";

    let table = build(input);
    let x = binding_at(&table, 20);
    assert_eq!(table.binding(x).special(), SpecialBinding::DeclaredSpecial);
    assert!(!table.binding(x).is_propagatable());
}

#[test]
fn an_undeclared_binding_stays_lexical_however_it_is_spelled() {
    // Earmuffs are a convention, not a declaration; this layer records proofs.
    let input = "(let ((*x* 1)) *x*)";

    let table = build(input);
    assert_eq!(
        table.binding(binding_at(&table, 7)).special(),
        SpecialBinding::Lexical
    );
}

#[test]
fn a_binder_records_its_head_and_its_initial_value_form() {
    let input = "(let ((x (compute))) x)";

    let table = build(input);
    let binding = table.binding(binding_at(&table, 7));
    assert_eq!(binding.binder_head(), Some("let"));
    assert_eq!(
        binding.init_form().map(|span| span.slice(input)),
        Some("(compute)")
    );
}

#[test]
fn a_dolist_binds_its_variable_over_the_result_form_and_the_body() {
    let input = "(dolist (x items x) (use x))";

    let table = build(input);
    assert_eq!(
        reference_labels(&table, binding_at(&table, 9), input),
        vec!["x@17", "x@25"]
    );
}

#[test]
fn a_do_star_spec_reads_the_specs_before_it_but_a_do_spec_does_not() {
    let sequential = "(do* ((a 1) (b a)) (nil) b)";
    let table = build(sequential);
    assert_eq!(
        reference_labels(&table, binding_at(&table, 7), sequential),
        vec!["a@15"]
    );

    let parallel = "(do ((a 1) (b a)) (nil) b)";
    let table = build(parallel);
    assert!(reference_labels(&table, binding_at(&table, 6), parallel).is_empty());
}

#[test]
fn every_scope_hangs_off_the_file_scope() {
    let input = "(let ((a 1)) (let ((b 2)) (list a b)))";

    let table = build(input);
    let outer = table.binding(binding_at(&table, 7)).scope();
    let inner = table.binding(binding_at(&table, 20)).scope();
    assert!(table.is_within(inner, outer));
    assert!(!table.is_within(outer, inner));
    assert!(table.is_within(outer, crate::semantics::binding::ScopeId::FILE));
}
