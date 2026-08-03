#![doc = include_str!("../README.md")]

pub mod corpus;
pub mod defstruct_boa_aux_uninitialized_slot;
pub mod defstruct_include_type_mismatch;
pub mod hash_table_literal_string_key_under_eql;
pub mod make_array_conflicting_initializers;
pub mod maphash_mutates_other_entry;
pub mod support;
pub mod vector_push_without_fill_pointer;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary; this package is deliberately left unregistered until a separate
// wiring pass adds it.

/// Runs the six rules through the engine itself, over a catalogue built here.
///
/// Every other test in this package calls a rule's `examine`/`build_*_report`
/// directly, which walks the whole tree and so proves nothing about the one
/// declaration a `HeadFilter::Heads` rule lives or dies by. A rule whose
/// declared heads do not match what its `examine` looks for compiles, passes
/// every direct test, and is then never invoked by the real dispatcher on any
/// file.
///
/// The catalogue is local so the assertion can be made here, on this package's
/// six rules and nothing else.
#[cfg(test)]
mod engine_dispatch {
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    static ENTRIES: [RuleEntry; 6] = [
        RuleEntry::new(
            &crate::defstruct_boa_aux_uninitialized_slot::rule::META,
            &crate::defstruct_boa_aux_uninitialized_slot::rule::RULE,
        ),
        RuleEntry::new(
            &crate::defstruct_include_type_mismatch::rule::META,
            &crate::defstruct_include_type_mismatch::rule::RULE,
        ),
        RuleEntry::new(
            &crate::hash_table_literal_string_key_under_eql::rule::META,
            &crate::hash_table_literal_string_key_under_eql::rule::RULE,
        ),
        RuleEntry::new(
            &crate::make_array_conflicting_initializers::rule::META,
            &crate::make_array_conflicting_initializers::rule::RULE,
        ),
        RuleEntry::new(
            &crate::maphash_mutates_other_entry::rule::META,
            &crate::maphash_mutates_other_entry::rule::RULE,
        ),
        RuleEntry::new(
            &crate::vector_push_without_fill_pointer::rule::META,
            &crate::vector_push_without_fill_pointer::rule::RULE,
        ),
    ];

    const CATALOG: RuleCatalog = RuleCatalog::new(&ENTRIES);

    /// The rule names the engine reports for `source`, deduplicated and sorted.
    fn rules_fired(source: &str) -> Vec<&'static str> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let index = build_head_index(CATALOG);
        let mut fired: Vec<&'static str> = collect_lint_outcomes(
            CATALOG,
            &index,
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("the pass must succeed")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        fired.sort_unstable();
        fired.dedup();
        fired
    }

    /// One file that trips all six, so a rule whose declared heads do not reach
    /// its own `examine` fails here rather than silently never running.
    #[test]
    fn every_rule_is_reached_through_the_head_index() {
        let source = concat!(
            "(defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 999))\n",
            "(defstruct (base (:type list)) p q)\n",
            "(defstruct (derived (:include base)) r)\n",
            "(defparameter *cache* (make-hash-table))\n",
            "(defparameter *buf* (make-array 8))\n",
            "(defun f ()\n",
            "  (gethash \"alpha\" *cache*)\n",
            "  (vector-push-extend 1 *buf*)\n",
            "  (make-array 3 :initial-element 0 :initial-contents '(1 2 3))\n",
            "  (maphash (lambda (k v) (declare (ignore k v)) (clrhash *cache*)) *cache*))\n",
        );
        assert_eq!(
            rules_fired(source),
            vec![
                "defstruct-boa-aux-uninitialized-slot",
                "defstruct-include-type-mismatch",
                "hash-table-literal-string-key-under-eql",
                "make-array-conflicting-initializers",
                "maphash-mutates-other-entry",
                "vector-push-without-fill-pointer",
            ]
        );
    }

    /// The cost claim in the package README, as a test: a file with none of the
    /// seven anchor heads reaches no rule in this package at all.
    #[test]
    fn a_file_with_no_aggregate_form_trips_nothing() {
        assert!(rules_fired("(defun add (a b)\n  \"Return A plus B.\"\n  (+ a b))\n").is_empty());
    }

    /// Every rule's declared heads must be a subset of the seven this package
    /// anchors on — the head index is what keeps a clean file free, and
    /// `WholeTree` costs a pass on every file linted.
    #[test]
    fn no_rule_declares_a_whole_tree_or_all_nodes_filter() {
        use paredit_core_lint_engine::model::HeadFilter;

        const ANCHORS: [&str; 7] = [
            "defstruct",
            "gethash",
            "make-array",
            "maphash",
            "remhash",
            "vector-push",
            "vector-push-extend",
        ];

        for entry in CATALOG.entries() {
            let filter = entry.rule().head_filter();
            let HeadFilter::Heads(heads) = filter else {
                panic!(
                    "{} declares {filter:?}, which costs a pass on every file",
                    entry.meta().name().as_str()
                );
            };
            assert!(!heads.is_empty());
            for head in heads {
                assert!(
                    ANCHORS.contains(&head.as_str()),
                    "{} anchors on an unexpected head {}",
                    entry.meta().name().as_str(),
                    head.as_str()
                );
            }
        }
    }
}
