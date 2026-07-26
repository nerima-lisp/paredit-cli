//! The differential agreement, over generated nests of binding forms.
//!
//! The hand-written fixtures cover each binder once. What they cannot cover is
//! *nesting*: shadowing chains, a `let` initial value inside a `labels` body
//! inside a lambda list default. Those are where a scope stack goes wrong, and
//! where a generator beats a fixture list.

use proptest::prelude::*;

use crate::domain::dialect::Dialect;
use crate::domain::semantics::NodeKey;
use crate::domain::sexpr::SyntaxTree;

use super::build;
use super::differential::assert_partition;

/// A deliberately tiny name alphabet, so shadowing happens constantly.
const NAMES: [&str; 3] = ["a", "b", "x"];

/// Nested Common Lisp binding forms.
///
/// Every generated form is one this layer claims to understand — the point is
/// to stress the *scope stack*, not the parser. Ordinary calls, quoting, and
/// quasiquotation are mixed in because they are what separates a live
/// reference from inert data.
fn binding_form_strategy() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        prop::sample::select(NAMES.as_slice()).prop_map(str::to_owned),
        Just("1".to_owned()),
    ];

    leaf.prop_recursive(4, 48, 3, |inner| {
        let name = prop::sample::select(NAMES.as_slice());
        prop_oneof![
            // Parallel and sequential value bindings.
            (name.clone(), inner.clone(), inner.clone())
                .prop_map(|(n, v, b)| format!("(let (({n} {v})) {b})")),
            (
                name.clone(),
                name.clone(),
                inner.clone(),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(n, m, v, w, b)| format!("(let* (({n} {v}) ({m} {w})) {b})")),
            (
                name.clone(),
                name.clone(),
                inner.clone(),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(n, m, v, w, b)| format!("(let (({n} {v}) ({m} {w})) {b})")),
            // Function-namespace bindings, which must not capture value
            // references of the same name.
            (name.clone(), name.clone(), inner.clone(), inner.clone())
                .prop_map(|(f, p, d, b)| format!("(flet (({f} ({p}) {d})) {b})")),
            (name.clone(), name.clone(), inner.clone(), inner.clone())
                .prop_map(|(f, p, d, b)| format!("(labels (({f} ({p}) {d})) {b})")),
            // Lambda lists, including a default that must read leftwards only.
            (name.clone(), inner.clone()).prop_map(|(p, b)| format!("(lambda ({p}) {b})")),
            (name.clone(), name.clone(), inner.clone(), inner.clone())
                .prop_map(|(p, q, d, b)| format!("(lambda ({p} &optional ({q} {d})) {b})")),
            // Iteration and stepping.
            (name.clone(), inner.clone(), inner.clone())
                .prop_map(|(n, s, b)| format!("(dolist ({n} {s}) {b})")),
            (name.clone(), inner.clone(), inner.clone(), inner.clone())
                .prop_map(|(n, i, s, b)| format!("(do* (({n} {i} {s})) (nil) {b})")),
            (name.clone(), inner.clone(), inner.clone())
                .prop_map(|(n, v, b)| format!("(multiple-value-bind ({n}) {v} {b})")),
            // Reassignment, which must land on the innermost binding.
            (name.clone(), inner.clone()).prop_map(|(n, v)| format!("(setq {n} {v})")),
            (name.clone(), inner.clone()).prop_map(|(n, v)| format!("(incf {n} {v})")),
            (name, inner.clone()).prop_map(|(n, v)| format!("(setf (car {n}) {v})")),
            // Ordinary calls and inert data.
            prop::collection::vec(inner.clone(), 1..3)
                .prop_map(|forms| format!("(call {})", forms.join(" "))),
            inner.clone().prop_map(|form| format!("'{form}")),
            inner.clone().prop_map(|form| format!("`(tag ,{form})")),
            inner.prop_map(|form| format!("#'{form}")),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The whole acceptance criterion, over generated input: on every file,
    /// the reference query and the binding table partition the same atoms.
    #[test]
    fn pbt_the_query_and_the_table_agree_on_nested_binders(input in binding_form_strategy()) {
        assert_partition(&input);
    }

    /// A reference is only ever attributed to a binding whose scope encloses
    /// it, and the two directions of the resolution stay consistent.
    #[test]
    fn pbt_every_reference_is_inside_its_scope(input in binding_form_strategy()) {
        prop_assume!(SyntaxTree::parse_with_dialect(&input, Dialect::CommonLisp).is_ok());
        let table = build(&input);

        for (id, binding) in table.bindings() {
            let opener = table
                .scope(binding.scope())
                .opener()
                .expect("a binding registered by the walk always has an opener");

            for span in binding.references() {
                prop_assert_eq!(table.resolve(NodeKey::atom(*span)), Some(id));
                prop_assert!(opener.start() <= span.start() && span.end() <= opener.end());
            }
            for span in binding.assignments() {
                prop_assert!(opener.start() <= span.start() && span.end() <= opener.end());
            }
        }
    }

    /// A binding that anything reassigns never propagates, however deeply the
    /// assignment is nested.
    #[test]
    fn pbt_a_reassigned_binding_never_propagates(input in binding_form_strategy()) {
        prop_assume!(SyntaxTree::parse_with_dialect(&input, Dialect::CommonLisp).is_ok());
        let table = build(&input);

        for (_, binding) in table.bindings() {
            if !binding.assignments().is_empty() {
                prop_assert!(!binding.is_propagatable());
            }
        }
    }
}
