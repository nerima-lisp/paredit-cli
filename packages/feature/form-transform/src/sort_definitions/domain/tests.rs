use std::path::PathBuf;

use proptest::{prelude::*, test_runner::TestCaseError};

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SyntaxTree;

use super::*;

fn request(input: &str, strategy: SortDefinitionsStrategy) -> SortDefinitionsRequest<'_> {
    request_for_dialect(input, Dialect::CommonLisp, strategy)
}

fn request_for_dialect(
    input: &str,
    dialect: Dialect,
    strategy: SortDefinitionsStrategy,
) -> SortDefinitionsRequest<'_> {
    SortDefinitionsRequest {
        file: PathBuf::from("core.lisp"),
        input,
        dialect,
        strategy,
        write: false,
    }
}

#[test]
fn sorts_contiguous_definitions_by_name() {
    let input = "(in-package #:demo)\n\n\
                 (defun zeta () :z)\n\
                 (defmacro alpha () nil)\n\
                 (defun beta () :b)\n";

    let plan = plan_sort_definitions(request(input, SortDefinitionsStrategy::Name))
        .expect("sort plan should be built");

    assert!(plan.changed);
    assert_eq!(plan.items.len(), 3);
    let alpha = plan.rewritten.find("(defmacro alpha").expect("alpha");
    let beta = plan.rewritten.find("(defun beta").expect("beta");
    let zeta = plan.rewritten.find("(defun zeta").expect("zeta");
    assert!(alpha < beta);
    assert!(beta < zeta);
    assert!(SyntaxTree::parse(&plan.rewritten).is_ok());
}

#[test]
fn sorts_definitions_for_every_known_dialect_and_reparses_with_the_same_dialect() {
    let fixtures = [
        (
            Dialect::CommonLisp,
            "(in-package #:demo)\n\n(defun zeta () :zeta)\n(defun alpha () :alpha)\n",
            "(defun alpha",
            "(defun zeta",
        ),
        (
            Dialect::EmacsLisp,
            "(defun zeta () 'zeta)\n(defun alpha () 'alpha)\n",
            "(defun alpha",
            "(defun zeta",
        ),
        (
            Dialect::Scheme,
            "(define zeta (lambda () 'zeta))\n(define alpha (lambda () 'alpha))\n",
            "(define alpha",
            "(define zeta",
        ),
        (
            Dialect::Clojure,
            "(defn zeta [] #?(:clj :zeta :cljs :zeta))\n(defn alpha [] :alpha)\n",
            "(defn alpha",
            "(defn zeta",
        ),
        (
            Dialect::Janet,
            "(defn zeta [] :zeta)\n(defn alpha [] :alpha)\n",
            "(defn alpha",
            "(defn zeta",
        ),
        (
            Dialect::Fennel,
            "(fn zeta [] :zeta)\n(fn alpha [] :alpha)\n",
            "(fn alpha",
            "(fn zeta",
        ),
    ];

    for (dialect, input, alpha_marker, zeta_marker) in fixtures {
        let plan = plan_sort_definitions(request_for_dialect(
            input,
            dialect,
            SortDefinitionsStrategy::Name,
        ))
        .unwrap_or_else(|error| panic!("{dialect:?} sort failed: {error:#}"));

        assert!(plan.changed, "{dialect:?} fixture was not sorted");
        assert!(
            plan.rewritten.find(alpha_marker).unwrap() < plan.rewritten.find(zeta_marker).unwrap(),
            "{dialect:?} output was not sorted: {}",
            plan.rewritten
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
            .unwrap_or_else(|error| panic!("{dialect:?} output did not reparse: {error:#}"));
    }
}

#[test]
fn rejects_unknown_dialect_before_parsing_malformed_input() {
    let error = plan_sort_definitions(request_for_dialect(
        "(",
        Dialect::Unknown,
        SortDefinitionsStrategy::Name,
    ))
    .unwrap_err();

    assert!(
        error.to_string().contains("unknown dialect"),
        "unexpected gate error: {error:#}"
    );
}

#[test]
fn does_not_cross_non_definition_barriers() {
    let input = "(defun zeta () :z)\n\
                 (print :barrier)\n\
                 (defun alpha () :a)\n";

    let plan = plan_sort_definitions(request(input, SortDefinitionsStrategy::Name))
        .expect("barrier plan should be built");

    assert!(!plan.changed);
    assert!(plan.items.is_empty());
    assert_eq!(plan.rewritten, input);
}

#[test]
fn kind_then_name_groups_categories_before_names() {
    let input = "(defmacro alpha () nil)\n\
                 (defun zeta () :z)\n\
                 (defun beta () :b)\n\
                 (define-symbol-macro current-user (session-user *session*))\n";

    let plan = plan_sort_definitions(request(input, SortDefinitionsStrategy::KindThenName))
        .expect("sort plan should be built");

    let beta = plan.rewritten.find("(defun beta").expect("beta");
    let zeta = plan.rewritten.find("(defun zeta").expect("zeta");
    let alpha = plan.rewritten.find("(defmacro alpha").expect("alpha");
    let current_user = plan
        .rewritten
        .find("(define-symbol-macro current-user")
        .expect("current-user");
    assert!(beta < zeta);
    assert!(zeta < alpha);
    assert!(alpha < current_user);
}

#[test]
fn sort_definitions_keeps_leading_comment_with_its_definition() {
    let input = "(defun zeta () :z)\n\
                 \n\
                 ;; alpha helper\n\
                 (defun alpha () :a)\n\
                 \n\
                 (defun beta () :b)\n";

    let plan = plan_sort_definitions(request(input, SortDefinitionsStrategy::Name))
        .expect("sort plan should be built");

    assert!(plan.changed);
    assert!(SyntaxTree::parse(&plan.rewritten).is_ok());
    let comment = plan.rewritten.find(";; alpha helper").expect("comment");
    let alpha = plan.rewritten.find("(defun alpha").expect("alpha");
    let beta = plan.rewritten.find("(defun beta").expect("beta");
    let zeta = plan.rewritten.find("(defun zeta").expect("zeta");
    assert!(
        comment < alpha && alpha - comment < 30,
        "comment should sit directly above the definition it describes"
    );
    assert!(alpha < beta);
    assert!(beta < zeta);
}

#[test]
fn sort_definitions_relocating_original_first_entry_has_no_stray_gap() {
    // zeta is the block's original first entry (no leading trivia of its
    // own) and sorts last by name, so it must pick up a clean separator
    // instead of gluing onto the previous definition's closing paren.
    let input = "(defun zeta () :z)\n\
                 (defun alpha () :a)\n";

    let plan = plan_sort_definitions(request(input, SortDefinitionsStrategy::Name))
        .expect("sort plan should be built");

    assert!(plan.changed);
    assert!(SyntaxTree::parse(&plan.rewritten).is_ok());
    assert!(
        !plan.rewritten.contains(")(defun"),
        "reordered definitions must not be glued together: {:?}",
        plan.rewritten
    );
}

fn symbol_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,10}".prop_map(|name| name)
}

fn assert_sorted_definitions_property(names: Vec<String>) -> Result<(), TestCaseError> {
    let mut reversed = names.clone();
    reversed.reverse();
    let mut input = String::from("(in-package #:demo)\n");
    for name in &reversed {
        input.push_str(&format!("(defun {name} () :ok)\n"));
    }

    let plan = plan_sort_definitions(request(&input, SortDefinitionsStrategy::Name))
        .map_err(|err| TestCaseError::fail(format!("sort plan: {err}")))?;
    prop_assert!(SyntaxTree::parse(&plan.rewritten).is_ok());

    let mut previous_position = 0;
    for name in names {
        let position = plan
            .rewritten
            .find(&format!("(defun {name}"))
            .ok_or_else(|| TestCaseError::fail(format!("missing {name}")))?;
        prop_assert!(position >= previous_position);
        previous_position = position;
    }

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn sorted_output_is_parseable_and_name_ordered(
        names in prop::collection::btree_set(symbol_name(), 2..8)
            .prop_map(|names| names.into_iter().collect::<Vec<_>>())
    ) {
        assert_sorted_definitions_property(names)?;
    }
}
