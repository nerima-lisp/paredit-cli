#![doc = include_str!("../README.md")]

pub mod around_method_missing_call_next_method;
pub mod defclass_required_slot_no_initform_or_initarg;
pub mod defclass_slot_shadowing;
pub mod duplicate_defmethod_signature;
pub mod generic_function_no_methods;
pub mod method_qualifier_typo;
pub mod print_object_without_print_unreadable_object;
pub mod slot_value_bypasses_accessor;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Runs the eight rules through the engine itself, over a catalogue built here.
///
/// Every other test in this package calls a rule's `examine`/`build_*_report`
/// directly, which is the shape the suite's rules are tested in — but that path
/// walks the whole tree and so proves nothing about the one declaration a
/// `HeadFilter::Heads` rule lives or dies by. A rule whose declared heads do not
/// match what its `examine` looks for compiles, passes every direct test, and
/// is then never invoked by the real dispatcher on any file.
///
/// The catalogue is local so the assertion can be made *here*, next to the
/// rules, on this package's eight and nothing else. The root's `REGISTRY` does
/// name all eight, but a run through it would put ~190 other rules' findings in
/// the way of the one question this asks — whether each `HeadFilter::Heads`
/// declaration actually reaches its own `examine` — and the head-subset
/// assertion below would have to enumerate every head in the tree to say
/// anything. Building a catalogue of exactly these eight keeps both tests
/// closed over this package. The cost is that the list is a second copy of the
/// registry's entries for this package, which is its own ticket.
#[cfg(test)]
mod engine_dispatch {
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    static ENTRIES: [RuleEntry; 8] = [
        RuleEntry::new(
            &crate::around_method_missing_call_next_method::rule::META,
            &crate::around_method_missing_call_next_method::rule::RULE,
        ),
        RuleEntry::new(
            &crate::defclass_required_slot_no_initform_or_initarg::rule::META,
            &crate::defclass_required_slot_no_initform_or_initarg::rule::RULE,
        ),
        RuleEntry::new(
            &crate::defclass_slot_shadowing::rule::META,
            &crate::defclass_slot_shadowing::rule::RULE,
        ),
        RuleEntry::new(
            &crate::duplicate_defmethod_signature::rule::META,
            &crate::duplicate_defmethod_signature::rule::RULE,
        ),
        RuleEntry::new(
            &crate::generic_function_no_methods::rule::META,
            &crate::generic_function_no_methods::rule::RULE,
        ),
        RuleEntry::new(
            &crate::method_qualifier_typo::rule::META,
            &crate::method_qualifier_typo::rule::RULE,
        ),
        RuleEntry::new(
            &crate::print_object_without_print_unreadable_object::rule::META,
            &crate::print_object_without_print_unreadable_object::rule::RULE,
        ),
        RuleEntry::new(
            &crate::slot_value_bypasses_accessor::rule::META,
            &crate::slot_value_bypasses_accessor::rule::RULE,
        ),
    ];

    const CATALOG: RuleCatalog = RuleCatalog::new(&ENTRIES);

    /// The rule names the engine reports for `source`, in report order.
    fn rules_fired(source: &str) -> Vec<&'static str> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let index = build_head_index(CATALOG);
        collect_lint_outcomes(
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
        .collect()
    }

    /// One file that trips all eight, so a rule whose declared heads do not
    /// reach its own `examine` fails here rather than silently never running.
    #[test]
    fn every_rule_is_reached_through_the_head_index() {
        let source = concat!(
            "(defclass shape () ((origin :initform 0 :initarg :origin)))\n",
            "(defclass circle (shape) ((origin :accessor origin-of) (radius)))\n",
            "(defgeneric erase (s))\n",
            "(defmethod area ((c circle)) (slot-value c 'origin))\n",
            "(defmethod draw :around ((c circle)) (log-it c))\n",
            "(defmethod draw :arround ((c shape)) c)\n",
            "(defmethod print-object ((c circle) s) (format s \"circle\"))\n",
            "(defmethod area ((c circle)) (slot-value c 'origin))\n",
        );
        let mut fired = rules_fired(source);
        fired.sort_unstable();
        fired.dedup();
        assert_eq!(
            fired,
            vec![
                "around-method-missing-call-next-method",
                "defclass-required-slot-no-initform-or-initarg",
                "defclass-slot-shadowing",
                "duplicate-defmethod-signature",
                "generic-function-no-methods",
                "method-qualifier-typo",
                "print-object-without-print-unreadable-object",
                "slot-value-bypasses-accessor",
            ]
        );
    }

    /// The cost claim in the package README, as a test: a file with none of the
    /// four anchor heads reaches no rule in this package at all.
    #[test]
    fn a_file_with_no_clos_form_trips_nothing() {
        assert!(rules_fired("(defun add (a b)\n  \"Return A plus B.\"\n  (+ a b))\n").is_empty());
    }

    /// Every rule's declared heads must be a subset of the four this package
    /// anchors on — the head index is what keeps a clean file free.
    #[test]
    fn no_rule_declares_a_whole_tree_or_all_nodes_filter() {
        use paredit_core_lint_engine::model::HeadFilter;

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
                    ["defclass", "defgeneric", "defmethod", "slot-value"].contains(&head.as_str()),
                    "{} anchors on an unexpected head {}",
                    entry.meta().name().as_str(),
                    head.as_str()
                );
            }
        }
    }
}
