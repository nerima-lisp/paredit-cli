#![doc = include_str!("../README.md")]

pub mod atom_swap_with_side_effect;
pub mod dynamic_var_bound_across_thread_boundary;
pub mod future_promise_never_realized;
pub mod lock_acquired_not_released;
pub mod recursive_lock_reentry_risk;
pub mod support;
pub mod thread_spawned_without_error_handler;
pub mod unsynchronized_shared_mutation;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// Two declarations decide whether a rule is reachable from the CLI at all, and
/// neither is visible to a domain test, which calls `examine_*` on a node it
/// picked itself:
///
/// - the `HeadFilter::Heads` list, which is what the dispatcher's head index is
///   built from. A head spelled wrongly — or a head the domain matches but the
///   list omits — leaves every `examine_*` test green while the rule never
///   receives a single node in production.
/// - the [`RuleDialectScope`], which the dispatcher consults *before* walking
///   anything. Five of these rules take the trait default (Common Lisp only);
///   `atom-swap-with-side-effect` and `future-promise-never-realized` are the
///   codebase's first two `CLOJURE_ONLY` rules, so that arm of the dispatch has
///   no other built-in-rule coverage anywhere.
///
/// [`RuleDialectScope`]: paredit_core_lint_engine::policy::RuleDialectScope
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::model::HeadFilter;
    use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 7] = [
        RuleEntry::new(
            &crate::atom_swap_with_side_effect::rule::META,
            &crate::atom_swap_with_side_effect::rule::RULE,
        ),
        RuleEntry::new(
            &crate::dynamic_var_bound_across_thread_boundary::rule::META,
            &crate::dynamic_var_bound_across_thread_boundary::rule::RULE,
        ),
        RuleEntry::new(
            &crate::future_promise_never_realized::rule::META,
            &crate::future_promise_never_realized::rule::RULE,
        ),
        RuleEntry::new(
            &crate::lock_acquired_not_released::rule::META,
            &crate::lock_acquired_not_released::rule::RULE,
        ),
        RuleEntry::new(
            &crate::recursive_lock_reentry_risk::rule::META,
            &crate::recursive_lock_reentry_risk::rule::RULE,
        ),
        RuleEntry::new(
            &crate::thread_spawned_without_error_handler::rule::META,
            &crate::thread_spawned_without_error_handler::rule::RULE,
        ),
        RuleEntry::new(
            &crate::unsynchronized_shared_mutation::rule::META,
            &crate::unsynchronized_shared_mutation::rule::RULE,
        ),
    ];

    /// A Common Lisp source that triggers exactly one of these rules, per rule.
    /// Three of the seven share the `make-thread` head, so each of the three is
    /// written to miss the other two: one inlined form (never a
    /// `thread-spawned-without-error-handler`), no earmuffed write (never an
    /// `unsynchronized-shared-mutation`), no enclosing rebinding (never a
    /// `dynamic-var-bound-across-thread-boundary`).
    const COMMON_LISP_TRIGGERS: [(&str, &str); 5] = [
        (
            "dynamic-var-bound-across-thread-boundary",
            "(let ((*database* (connect-to-replica)))\n  (bt:make-thread (lambda () (query *database*))))",
        ),
        (
            "lock-acquired-not-released",
            "(defun f () (bt:acquire-lock *l*) (work))",
        ),
        (
            "recursive-lock-reentry-risk",
            "(bt:with-lock-held (*l*) (work) (bt:with-lock-held (*l*) (more)))",
        ),
        (
            "thread-spawned-without-error-handler",
            "(bt:make-thread (lambda () (connect) (serve)))",
        ),
        (
            "unsynchronized-shared-mutation",
            "(bt:make-thread (lambda () (incf *counter*)))",
        ),
    ];

    /// The same, for the two Clojure rules.
    const CLOJURE_TRIGGERS: [(&str, &str); 2] = [
        (
            "atom-swap-with-side-effect",
            "(swap! log (fn [l] (println \"adding\") (conj l x)))",
        ),
        (
            "future-promise-never-realized",
            "(let [f (future (risky))] (other-work))",
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    // -- (a) each rule is reached through the head index ----------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        for (rule, source) in COMMON_LISP_TRIGGERS {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                vec![rule],
                "{rule} is unreachable through the head index"
            );
        }
        for (rule, source) in CLOJURE_TRIGGERS {
            assert_eq!(
                fired(source, Dialect::Clojure),
                vec![rule],
                "{rule} is unreachable through the head index"
            );
        }
    }

    /// Seven rules, seven distinct names: a copy-paste in `ENTRIES` that
    /// registered one slice twice would otherwise leave the loop above green.
    #[test]
    fn the_catalog_holds_all_seven_rules_once_each() {
        let mut names: Vec<&'static str> = RuleCatalog::new(&ENTRIES)
            .entries()
            .iter()
            .map(|entry| entry.meta().name().as_str())
            .collect();
        assert_eq!(names.len(), 7);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 7);
    }

    // -- (b) a file with none of these heads trips nothing --------------------

    /// What the `clean/forms/*` benchmarks measure: ordinary code, none of it
    /// concurrent, must not reach a single `check` body.
    #[test]
    fn a_file_with_none_of_these_heads_produces_no_findings() {
        let common_lisp = "(defpackage :app (:use :cl))\n(in-package :app)\n\
             (defvar *counter* 0)\n\
             (defun add (a b) (+ a b))\n\
             (defmethod render ((x integer) stream) (format stream \"~d\" x))\n\
             (let ((total 0)) (dolist (n '(1 2 3)) (incf total n)) total)\n";
        assert_eq!(fired(common_lisp, Dialect::CommonLisp), Vec::<&str>::new());

        let clojure = "(ns app.core)\n\
             (def state (atom 0))\n\
             (defn add [a b] (+ a b))\n\
             (reset! state 1)\n\
             (deref state)\n";
        assert_eq!(fired(clojure, Dialect::Clojure), Vec::<&str>::new());
    }

    /// Correct concurrent code, which is the file a reviewer runs first: every
    /// head here *does* match, so this exercises the `check` bodies rather than
    /// the index.
    #[test]
    fn correct_concurrent_code_produces_no_findings() {
        let common_lisp = "(defun start (queue)\n  \
             (bt:make-thread\n   \
               (lambda ()\n     \
                 (handler-case (loop (process (dequeue queue)))\n       \
                   (error (e) (log-error e))))))\n\n\
             (defun bump ()\n  \
               (bt:with-lock-held (*registry-lock*) (incf *registry-count*)))\n\n\
             (defun drain ()\n  \
               (bt:acquire-lock *l*)\n  \
               (unwind-protect (work) (bt:release-lock *l*)))\n";
        assert_eq!(fired(common_lisp, Dialect::CommonLisp), Vec::<&str>::new());

        let clojure = "(ns app.worker)\n\n\
             (defn tally [log x] (swap! log conj x))\n\n\
             (defn fan-out []\n  \
               (let [users (future (fetch-users))]\n    \
                 {:users @users}))\n";
        assert_eq!(fired(clojure, Dialect::Clojure), Vec::<&str>::new());
    }

    /// The dispatcher hands a rule every head-matched node, quoted data
    /// included; each `check` calls `support::is_unevaluated_at` to decline
    /// those. Nothing on the report path exercises that call.
    #[test]
    fn no_rule_fires_on_quoted_or_templated_code() {
        for source in COMMON_LISP_TRIGGERS.map(|(_, source)| source) {
            assert_eq!(
                fired(&format!("'{source}"), Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
            assert_eq!(
                fired(&format!("(defmacro m () `{source})"), Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is a macro template"
            );
        }
        for source in CLOJURE_TRIGGERS.map(|(_, source)| source) {
            assert_eq!(
                fired(&format!("'{source}"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    // -- (c) the dialect scope, including the first two CLOJURE_ONLY rules ----

    /// `RuleDialectScope::CLOJURE_ONLY`, the half that fires: these are the
    /// codebase's first two users of it, so this is the only built-in-rule
    /// coverage that arm has.
    #[test]
    fn the_clojure_only_rules_fire_on_clojure_input() {
        for (rule, source) in CLOJURE_TRIGGERS {
            assert_eq!(fired(source, Dialect::Clojure), vec![rule]);
        }
    }

    /// The half that must *not* fire, on bytes the Common Lisp reader accepts.
    ///
    /// `atom-swap-with-side-effect` is the one of the two that can be written
    /// in syntax both readers take: `fn`'s parameter vector is optional to the
    /// rule (the body scan starts after it if it is there, at index 1 if it is
    /// not), so this source *does* fire as Clojure — the assertion pair below
    /// isolates the scope as the only difference.
    #[test]
    fn the_clojure_only_swap_rule_is_silent_on_the_same_bytes_read_as_common_lisp() {
        let dual = "(swap! log (fn (l) (println \"adding\") (conj l x)))";
        assert_eq!(
            fired(dual, Dialect::Clojure),
            vec!["atom-swap-with-side-effect"]
        );
        assert_eq!(fired(dual, Dialect::CommonLisp), Vec::<&str>::new());
    }

    /// `future-promise-never-realized` has no dual-syntax source at all: it
    /// requires a `[…]` binding vector, which Common Lisp has no `let` syntax
    /// for. `[` and `]` are constituent characters in Common Lisp (CLHS
    /// 2.4.2), not delimiters, so the Clojure spelling still *parses* as
    /// Common Lisp — just not as anything shaped like the `let` this rule
    /// looks for, since `[f` and `]` read as ordinary symbols rather than
    /// opening and closing a binding form. So its Common Lisp silence is
    /// pinned two ways — the Clojure spelling does not produce a finding once
    /// read as Common Lisp, and the Common Lisp `let` spelling that *is*
    /// head-matched by the rule's `HeadFilter::Heads` reaches no finding
    /// either.
    #[test]
    fn the_clojure_only_future_rule_is_silent_on_common_lisp() {
        let clojure = "(let [f (future (risky))] (other-work))";
        assert!(SyntaxTree::parse_with_dialect(clojure, Dialect::CommonLisp).is_ok());
        assert_eq!(fired(clojure, Dialect::CommonLisp), Vec::<&str>::new());

        let common_lisp = "(let ((f (future (risky)))) (other-work))";
        assert_eq!(fired(common_lisp, Dialect::CommonLisp), Vec::<&str>::new());
    }

    #[test]
    fn the_common_lisp_rules_are_silent_on_clojure() {
        for (rule, source) in COMMON_LISP_TRIGGERS {
            assert_eq!(
                fired(source, Dialect::Clojure),
                Vec::<&str>::new(),
                "{rule} is Common Lisp only"
            );
        }
    }

    /// No rule here is meaningful in a dialect with neither `bordeaux-threads`
    /// nor Clojure reference types.
    #[test]
    fn no_rule_runs_in_an_unrelated_dialect() {
        for (_, source) in COMMON_LISP_TRIGGERS.into_iter().chain(CLOJURE_TRIGGERS) {
            for dialect in [Dialect::Scheme, Dialect::Racket, Dialect::EmacsLisp] {
                assert_eq!(fired(source, dialect), Vec::<&str>::new());
            }
        }
    }

    /// The scopes as declarations, so a slice that loses its
    /// `dialect_scope` override fails here and not only through a source
    /// sample that might stop triggering for some other reason.
    #[test]
    fn each_rule_declares_the_scope_its_vocabulary_belongs_to() {
        let clojure_only = [
            "atom-swap-with-side-effect",
            "future-promise-never-realized",
        ];
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            let expected = if clojure_only.contains(&entry.meta().name().as_str()) {
                RuleDialectScope::CLOJURE_ONLY
            } else {
                RuleDialectScope::COMMON_LISP_ONLY
            };
            assert_eq!(
                entry.rule().dialect_scope(),
                expected,
                "{} declares the wrong dialect scope",
                entry.meta().name().as_str()
            );
        }
    }

    // -- (d) no rule declares anything but Heads ------------------------------

    /// `AllNodes` and `WholeTree` are paid for on every file whether or not a
    /// rule matches, which is exactly what the zero-finding benchmarks measure.
    /// The README states this as a package-wide property; this is the test that
    /// makes it one.
    #[test]
    fn every_rule_declares_a_non_empty_heads_filter() {
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            let name = entry.meta().name().as_str();
            let HeadFilter::Heads(heads) = entry.rule().head_filter() else {
                panic!("{name} declares something other than HeadFilter::Heads");
            };
            assert!(!heads.is_empty(), "{name} declares an empty head list");
        }
    }
}
