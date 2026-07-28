use super::*;
use paredit_core_syntax::dialect::Dialect;

fn reference_texts_for(dialect: Dialect, input: &str, symbol: &str) -> Vec<String> {
    let view = selected_form_with_dialect(input, dialect);
    let symbol = SymbolName::new(symbol).expect("symbol");
    let mut spans = Vec::new();
    collect_unshadowed_symbol_references(dialect, &view, &symbol, input, &mut spans);
    spans
        .into_iter()
        .map(|span| span.slice(input).to_owned())
        .collect()
}

/// How many references to `outer` survive, given that every fixture below
/// wraps the form under test between two occurrences that must always survive.
fn outer_reference_count(input: &str) -> usize {
    reference_texts_for(Dialect::Scheme, input, "outer").len()
}

#[test]
fn scheme_named_let_preserves_outer_references_outside_local_callable_body() {
    let input = "(list loop (let loop ((value loop)) (loop value)) loop)";

    assert_eq!(
        reference_texts_for(Dialect::Scheme, input, "loop"),
        vec!["loop", "loop", "loop"]
    );
}

#[test]
fn scheme_named_let_star_preserves_outer_references_in_binding_inits() {
    let input = "(list outer (let* loop ((value outer) (copy value)) (list copy)) outer)";

    assert_eq!(
        reference_texts_for(Dialect::Scheme, input, "outer"),
        vec!["outer", "outer", "outer"]
    );
}

#[test]
fn bracketed_binding_lists_are_read_like_parenthesised_ones() {
    // The dominant Racket spelling, and legal in every R6RS reader.
    let shadowed = "(list outer (let ([outer 1]) outer) outer)";
    let visible = "(list outer (let ([inner outer]) inner) outer)";

    assert_eq!(outer_reference_count(shadowed), 2);
    assert_eq!(outer_reference_count(visible), 3);
}

#[test]
fn letrec_makes_every_name_visible_to_every_initializer() {
    // `outer` is rebound, so the reference inside the *initializer* belongs to
    // the inner binding -- unlike `let*`, where an earlier initializer still
    // sees the outer one.
    let input = "(list outer (letrec ((f (lambda () outer)) (outer 1)) (f)) outer)";

    assert_eq!(outer_reference_count(input), 2);
}

#[test]
fn let_star_still_exposes_initializers_written_before_the_shadowing_entry() {
    let input = "(list outer (let* ((copy outer) (outer 1)) copy) outer)";

    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn parallel_let_evaluates_every_initializer_outside_its_own_scope() {
    let input = "(list outer (let ((a outer) (outer 1)) a) outer)";

    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn letrec_star_shares_letrec_visibility() {
    let input = "(list outer (letrec* ((f (lambda () outer)) (outer 1)) (f)) outer)";

    assert_eq!(outer_reference_count(input), 2);
}

#[test]
fn let_values_binds_every_name_in_its_formals_list() {
    let shadowed = "(list outer (let-values (((a outer) (produce))) a) outer)";
    let visible = "(list outer (let-values (((a b) outer)) a) outer)";

    assert_eq!(outer_reference_count(shadowed), 2);
    assert_eq!(outer_reference_count(visible), 3);
}

#[test]
fn let_star_values_exposes_earlier_producers() {
    let input = "(list outer (let*-values (((a) outer) ((outer) (produce))) a) outer)";

    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn do_loop_variables_shadow_the_step_test_and_body_but_not_the_initializer() {
    let shadowed = "(list outer (do ((outer 0 (+ outer 1))) ((= outer 3) outer) (use outer)) outer)";
    // Only the two wrapping occurrences survive: the initializer `0` names
    // nothing, and every other occurrence is the loop variable.
    assert_eq!(outer_reference_count(shadowed), 2);

    let initializer = "(list outer (do ((i outer (+ i 1))) ((= i 3) i)) outer)";
    assert_eq!(outer_reference_count(initializer), 3);
}

#[test]
fn do_loop_reaches_references_in_step_test_and_result_forms() {
    let input = "(list outer (do ((i 0 (+ i outer))) ((= i outer) outer)) outer)";

    // Two wrappers plus the step, the test and the result form.
    assert_eq!(outer_reference_count(input), 5);
}

#[test]
fn case_lambda_scopes_each_clause_independently() {
    let input = "(list outer (case-lambda ((outer) outer) ((a) outer)) outer)";

    // The first clause rebinds `outer`; the second does not.
    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn a_bare_symbol_lambda_formals_list_binds_every_argument() {
    let shadowed = "(list outer (lambda outer outer) outer)";
    let visible = "(list outer (lambda args outer) outer)";

    assert_eq!(outer_reference_count(shadowed), 2);
    assert_eq!(outer_reference_count(visible), 3);
}

#[test]
fn dotted_lambda_formals_bind_the_rest_parameter() {
    let shadowed = "(list outer (lambda (a . outer) outer) outer)";
    let visible = "(list outer (lambda (a . rest) outer) outer)";

    assert_eq!(outer_reference_count(shadowed), 2);
    assert_eq!(outer_reference_count(visible), 3);
}

#[test]
fn guard_binds_its_condition_variable_over_the_clauses_only() {
    // The guarded body runs before the handler exists, so `outer` there is the
    // outer binding even though the handler rebinds the same name.
    let input = "(list outer (guard (outer (#t outer)) (raise outer)) outer)";

    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn guard_clauses_see_references_that_are_not_the_condition_variable() {
    let input = "(list outer (guard (e (#t outer)) (raise 1)) outer)";

    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn parameterize_binds_nothing_so_both_halves_and_the_body_stay_visible() {
    // `(parameterize ((p v)) body)` rebinds a parameter object dynamically. The
    // left-hand side is a *reference*, not a new lexical name, so reading these
    // pairs as a binding list would have hidden the body.
    let input = "(parameterize ((outer outer)) outer)";

    assert_eq!(outer_reference_count(input), 3);
}

#[test]
fn fluid_let_binds_nothing_lexically_either() {
    let input = "(fluid-let ((outer 1)) outer)";

    assert_eq!(outer_reference_count(input), 2);
}

#[test]
fn define_parameters_shadow_the_body_without_the_procedure_name_counting() {
    let shadowed = "(define (f outer) outer)";
    assert_eq!(outer_reference_count(shadowed), 0);

    let visible = "(define (f a) outer)";
    assert_eq!(outer_reference_count(visible), 1);
}

#[test]
fn a_recursive_call_in_a_define_body_is_not_treated_as_shadowed() {
    // The procedure's own name shares a node with its parameters. Reading that
    // node as a plain parameter list made `f` look bound by itself, and every
    // recursive call vanished from the reference set.
    let input = "(define (outer n) (outer (- n 1)))";

    assert_eq!(reference_texts_for(Dialect::Scheme, input, "outer"), vec!["outer"]);
}

#[test]
fn curried_define_binds_the_parameters_of_every_level() {
    let inner = "(define ((adder n) outer) outer)";
    assert_eq!(outer_reference_count(inner), 0);

    let outer_level = "(define ((adder outer) x) outer)";
    assert_eq!(outer_reference_count(outer_level), 0);

    let neither = "(define ((adder n) x) outer)";
    assert_eq!(outer_reference_count(neither), 1);
}

#[test]
fn a_variable_define_evaluates_its_value_in_the_enclosing_scope() {
    let input = "(define x outer)";

    assert_eq!(reference_texts_for(Dialect::Scheme, input, "outer"), vec!["outer"]);
}

#[test]
fn define_syntax_bodies_are_left_to_the_expander() {
    // A transformer is code the macro expander runs, not code this file
    // evaluates, so a symbol inside it is not a reference here. Common Lisp's
    // `define-compiler-macro` is skipped for the same reason.
    let input = "(define-syntax m (syntax-rules () ((_ a) outer)))";

    assert!(reference_texts_for(Dialect::Scheme, input, "outer").is_empty());
}

#[test]
fn define_record_type_names_are_not_variable_references() {
    let input = "(define-record-type outer (outer a) outer? (a outer-a))";

    assert!(reference_texts_for(Dialect::Scheme, input, "outer").is_empty());
}

#[test]
fn let_syntax_shadows_like_let() {
    let shadowed = "(list outer (let-syntax ((outer (syntax-rules () ((_) 1)))) (outer)) outer)";
    assert_eq!(outer_reference_count(shadowed), 2);
}

#[test]
fn ordinary_control_forms_stay_transparent() {
    // `begin`, `if` and `cond` bind nothing, so every occurrence survives.
    let input = "(begin (if outer outer (cond (outer outer))))";

    assert_eq!(outer_reference_count(input), 4);
}

#[test]
fn racket_shares_the_scheme_binding_rules() {
    let input = "(list outer (letrec ([f (lambda () outer)] [outer 1]) (f)) outer)";

    assert_eq!(
        reference_texts_for(Dialect::Racket, input, "outer").len(),
        2
    );
}

#[test]
fn scheme_identifier_comparison_is_case_sensitive() {
    // R7RS 2.1 makes identifiers case-sensitive, so `Outer` never shadows
    // `outer` the way a Common Lisp symbol would.
    let input = "(list outer (let ((Outer 1)) outer) outer)";

    assert_eq!(outer_reference_count(input), 3);
}
