//! The dispatcher's behaviour over the *shipped* rule set.
//!
//! These live on the root side rather than in `paredit-core-lint-engine`
//! because every one of them asserts something about the real registry -
//! report order, dialect scoping, head-index coverage - and the engine package
//! deliberately has no registry to assert against (section 4.2). They drive
//! the engine only through its public entry point.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::lint::collect_lint_outcomes;
    use crate::lint::policy::RuleSelection;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// `self-assignment` (`WholeTree`) sits at registry position 0;
    /// `redundant-quote` (`AllNodes`) sits well after it. Both fire on this
    /// input, so the pair pins "registry order first" against a rule that is
    /// invoked ahead of the main walk.
    const MIXED_INPUT: &str = "(progn (setq x x) (list '5))";

    fn rule_names(input: &str, selection: RuleSelection<'_>) -> Vec<&'static str> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            selection,
        )
        .expect("collect lint outcomes")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
    }

    #[test]
    fn all_selection_reproduces_registry_order_across_interleaved_rules() {
        // Reconstructing report order from one interleaved pass is the whole
        // point of the dispatcher; a `WholeTree` rule (checked before the
        // walk) must still sort by its registry rank, not by when it ran.
        assert_eq!(
            rule_names(MIXED_INPUT, RuleSelection::All),
            vec!["self-assignment", "redundant-quote"]
        );
    }

    #[test]
    fn only_selection_excludes_unselected_rules_from_the_walk() {
        // A rule excluded from the selection must never be dispatched at
        // all, not merely filtered out of the result afterwards.
        assert_eq!(
            rule_names(MIXED_INPUT, RuleSelection::Only(&["self-assignment"])),
            vec!["self-assignment"]
        );
        assert_eq!(
            rule_names(MIXED_INPUT, RuleSelection::Only(&["redundant-quote"])),
            vec!["redundant-quote"]
        );
    }

    #[test]
    fn only_selection_withholds_the_fix_of_an_excluded_rule() {
        // `redundant-quote` is fixable; excluding it must drop the fix along
        // with the finding, since `--fix --rule other-rule` must never edit
        // text belonging to a rule the caller did not select.
        let input = "(list '5)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");

        let excluded = collect_lint_outcomes(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            RuleSelection::Only(&["self-assignment"]),
        )
        .expect("collect lint outcomes");
        assert!(excluded.is_empty());

        let included = collect_lint_outcomes(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            RuleSelection::Only(&["redundant-quote"]),
        )
        .expect("collect lint outcomes");
        assert_eq!(included.len(), 1);
        let (_, fix) = included
            .into_iter()
            .next()
            .expect("one outcome")
            .into_parts();
        assert!(
            fix.is_some(),
            "redundant-quote's fix must survive selection"
        );
    }

    #[test]
    fn a_dialect_no_rule_supports_short_circuits_to_no_outcomes() {
        // Every shipped rule is `COMMON_LISP_ONLY`, so `ActiveRules::any` is
        // false for any other dialect and the walk must never start.
        let tree = SyntaxTree::parse_with_dialect(MIXED_INPUT, Dialect::CommonLisp).expect("parse");
        let clojure_outcomes = collect_lint_outcomes(
            Path::new("test.clj"),
            Dialect::Clojure,
            &tree,
            MIXED_INPUT,
            RuleSelection::All,
        )
        .expect("collect lint outcomes");
        assert!(clojure_outcomes.is_empty());

        // The same source under Common Lisp still finds both rules, so the
        // empty result above is the dialect gate, not a parse artifact.
        assert_eq!(
            rule_names(MIXED_INPUT, RuleSelection::All),
            vec!["self-assignment", "redundant-quote"]
        );
    }

    #[test]
    fn a_head_scoped_rule_is_reached_through_every_case_spelling_the_head_index_promises() {
        // `zero-divisor` declares `HeadFilter::Heads(["mod", ...])` and its
        // own re-check is a plain `to_ascii_lowercase()` comparison. The
        // dispatcher's head-index key must fold case exactly the same way,
        // or the rule would be skipped for `MOD`/`Mod` even though it would
        // accept them itself — testing that agreement through the real
        // dispatcher is stronger than testing `head_index::head_key` in
        // isolation.
        for spelling in ["(mod x 0)", "(MOD x 0)", "(Mod x 0)"] {
            assert_eq!(
                rule_names(spelling, RuleSelection::Only(&["zero-divisor"])),
                vec!["zero-divisor"],
                "expected zero-divisor to fire for {spelling:?}"
            );
        }
    }

    #[test]
    fn the_head_index_over_approximates_package_qualifiers_and_reader_escapes_harmlessly() {
        // `head_key` also strips a package qualifier and resolves reader
        // escapes (see its doc comment), which is a deliberately *wider* net
        // than any current rule's own predicate needs — no shipped rule
        // understands `cl:`-qualified or `|...|`-escaped operator heads
        // itself, so the dispatcher reaching the candidate rule must still
        // end in zero findings rather than a false positive.
        for spelling in ["(CL:MOD x 0)", "(|MOD| x 0)"] {
            assert!(
                rule_names(spelling, RuleSelection::Only(&["zero-divisor"])).is_empty(),
                "expected zero-divisor to stay silent on {spelling:?}"
            );
        }
    }

    #[test]
    fn the_iterative_walk_reaches_a_deeply_nested_node_exactly_once() {
        // The dispatcher replaced a recursive per-rule walk with one
        // iterative, stack-based pre-order pass; a document nested well past
        // any reasonable call-stack depth must still be walked completely,
        // with the redundant quote found exactly once.
        let depth = 500;
        let input = format!("{}'5{}", "(progn ".repeat(depth), ")".repeat(depth));
        assert_eq!(
            rule_names(&input, RuleSelection::Only(&["redundant-quote"])),
            vec!["redundant-quote"]
        );
    }
}
