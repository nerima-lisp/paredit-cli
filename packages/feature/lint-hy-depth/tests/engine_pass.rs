//! Every rule driven through the *real* engine rather than by calling
//! `examine_try` directly.
//!
//! See `support/mod.rs` for why calling the domain directly cannot substitute
//! for this.

mod support;

use paredit_core_syntax::dialect::Dialect;

use support::{messages, rules_fired};

// -- the rule reaches the engine ------------------------------------------

#[test]
fn the_rule_fires_through_the_real_dispatch() {
    assert_eq!(
        rules_fired(
            "(try (f) (except [e Exception] 1) (except [e ValueError] 2))",
            Dialect::Hy
        ),
        vec!["hy-unreachable-except-clause"]
    );
}

/// The head index is keyed on `try`. A rule whose `Heads` lost that entry would
/// keep every unit test in `domain.rs` green and report nothing at all here.
#[test]
fn the_anchored_head_reaches_the_index() {
    assert_eq!(
        rules_fired(
            "(try (g) (except [] 1) (except [e KeyError] 2))",
            Dialect::Hy
        ),
        vec!["hy-unreachable-except-clause"]
    );
}

/// Two dead clauses in one `try` both arrive: the rule reports every one it
/// finds, and a rule that reported only the first would still pass the test
/// above.
#[test]
fn every_dead_clause_is_reported_not_just_the_first() {
    assert_eq!(
        rules_fired(
            "(try (f) (except [e Exception] 1) (except [e ValueError] 2) (except [e KeyError] 3))",
            Dialect::Hy
        ),
        vec![
            "hy-unreachable-except-clause",
            "hy-unreachable-except-clause"
        ]
    );
}

/// Two separate `try` forms in one file both arrive: the head index dispatches
/// per node.
#[test]
fn every_matching_node_is_dispatched_not_just_the_first() {
    assert_eq!(
        rules_fired(
            "(try (f) (except [e OSError] 1) (except [e OSError] 2))\n\
             (try (g) (except [e ValueError] 1) (except [e ValueError] 2))",
            Dialect::Hy
        ),
        vec![
            "hy-unreachable-except-clause",
            "hy-unreachable-except-clause"
        ]
    );
}

// -- the dialect scope, which only the engine applies ---------------------

/// The trait default is `COMMON_LISP_ONLY`. Without the override the rule would
/// never fire on Hy — and would fire on Common Lisp, where `(try …)` is not
/// this operator at all.
/// Every dialect that can *read* the fixture at all.
///
/// Common Lisp is absent because it cannot: `[` is not a delimiter there, so
/// `(except [e Exception] …)` is an `UnexpectedClose` and there is no Common
/// Lisp source at all on which this rule could fire — `caught_by` requires a
/// bracket list and Common Lisp cannot produce one.
///
/// That matters, because Common Lisp is precisely the dialect a *missing*
/// `dialect_scope()` would restrict the rule to. It is covered instead by
/// `the_rule_fires_through_the_real_dispatch` above: with the override removed
/// the scope becomes `COMMON_LISP_ONLY`, the rule stops firing on Hy, and that
/// test fails. Mutation-tested, and recorded in the README.
const READABLE_ELSEWHERE: [Dialect; 8] = [
    Dialect::Clojure,
    Dialect::Scheme,
    Dialect::Racket,
    Dialect::EmacsLisp,
    Dialect::Fennel,
    Dialect::Janet,
    Dialect::Lfe,
    Dialect::Carp,
];

#[test]
fn no_rule_fires_on_a_dialect_outside_its_scope() {
    let source = "(try (f) (except [e Exception] 1) (except [e ValueError] 2))";
    for dialect in READABLE_ELSEWHERE {
        // Asserting the parse succeeds is what stops this being a false clean:
        // a dialect that refused the source would report no findings for a
        // reason that has nothing to do with the dialect scope.
        assert!(
            paredit_core_syntax::sexpr::SyntaxTree::parse_with_dialect(source, dialect).is_ok(),
            "{dialect:?} no longer reads the fixture, so its zero findings prove nothing"
        );
        assert_eq!(
            rules_fired(source, dialect),
            Vec::<&str>::new(),
            "hy-unreachable-except-clause fired on {dialect:?}"
        );
    }
}

// -- the head comparison, which the index cannot make ---------------------

/// The head index ASCII-lowercases every head it stores, so it offers
/// `(TRY …)` to a rule registered for `try`. Hy is case sensitive, because
/// Python is, and `TRY` is an ordinary function name.
#[test]
fn an_upper_case_head_is_not_the_operator() {
    assert_eq!(
        rules_fired(
            "(TRY (f) (except [e Exception] 1) (except [e ValueError] 2))",
            Dialect::Hy
        ),
        Vec::<&str>::new()
    );
}

/// `#(...)` is Hy's *tuple*, a self-evaluating constant. It parses as a paren
/// list, so the index hands it to this rule as though it were a `try` form.
#[test]
fn a_tuple_constant_is_not_a_try_form() {
    assert_eq!(
        rules_fired(
            "#(try (f) (except [e Exception] 1) (except [e ValueError] 2))",
            Dialect::Hy
        ),
        Vec::<&str>::new()
    );
}

// -- the quote guard, which only the engine can exercise ------------------

/// The dispatcher descends into quoted data unconditionally, so each of these
/// reaches `check` and is rejected there. Without the guard each one fires.
#[test]
fn a_form_inside_a_macro_template_is_not_reported() {
    for source in [
        "(defmacro m [] `(do (try (f) (except [e Exception] 1) (except [e ValueError] 2))))",
        "(defmacro m [] '(do (try (f) (except [e Exception] 1) (except [e ValueError] 2))))",
        "(setv template (quote (try (f) (except [e Exception] 1) (except [e ValueError] 2))))",
    ] {
        assert_eq!(
            rules_fired(source, Dialect::Hy),
            Vec::<&str>::new(),
            "fired on {source}"
        );
    }
}

/// The template's own root carries the `Quasiquote` prefix, which makes it not
/// a call at all — a different path through the guard than the nested case
/// above, and one a rule relying only on `is_unevaluated_at` would still pass.
#[test]
fn a_quasiquoted_try_at_the_template_root_is_not_reported() {
    assert_eq!(
        rules_fired(
            "(defmacro m [] `(try (f) (except [e Exception] 1) (except [e ValueError] 2)))",
            Dialect::Hy
        ),
        Vec::<&str>::new()
    );
}

// -- what must stay clean -------------------------------------------------

#[test]
fn a_correctly_ordered_handler_chain_is_clean() {
    assert_eq!(
        rules_fired(
            "(try (f)\n\
             \x20 (except [e FileNotFoundError] 1)\n\
             \x20 (except [e OSError] 2)\n\
             \x20 (except [e Exception] 3)\n\
             \x20 (else 4)\n\
             \x20 (finally 5))",
            Dialect::Hy
        ),
        Vec::<&str>::new()
    );
}

/// The deliberate under-report, pinned so that widening it later is a choice
/// somebody makes rather than a regression.
#[test]
fn a_project_exception_class_is_never_called_shadowed_by_exception() {
    assert_eq!(
        rules_fired(
            "(try (f) (except [e Exception] 1) (except [e MyProjectError] 2))",
            Dialect::Hy
        ),
        Vec::<&str>::new()
    );
}

/// `BaseException` is the one supertype that provably covers a class this
/// layer has never heard of, so it is the one case that is reported.
#[test]
fn base_exception_does_shadow_a_project_exception_class() {
    assert_eq!(
        rules_fired(
            "(try (f) (except [e BaseException] 1) (except [e MyProjectError] 2))",
            Dialect::Hy
        ),
        vec!["hy-unreachable-except-clause"]
    );
}

// -- messages -------------------------------------------------------------

#[test]
fn the_message_names_the_shadowing_clause_and_why() {
    let same = messages(
        "(try (f) (except [e ValueError] 1) (except [e ValueError] 2))",
        Dialect::Hy,
    );
    assert!(same[0].contains("already names `ValueError`"), "{same:?}");

    let broader = messages(
        "(try (f) (except [e OSError] 1) (except [e FileNotFoundError] 2))",
        Dialect::Hy,
    );
    assert!(
        broader[0].contains("names `OSError`, which this one's type inherits from"),
        "{broader:?}"
    );

    let catch_all = messages(
        "(try (f) (except [] 1) (except [e ValueError] 2))",
        Dialect::Hy,
    );
    assert!(
        catch_all[0].contains("catches every exception"),
        "{catch_all:?}"
    );
}
