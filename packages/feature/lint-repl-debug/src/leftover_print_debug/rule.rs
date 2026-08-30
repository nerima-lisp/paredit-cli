//! `leftover-print-debug`: a bare debug-print call left in committed source.
//!
//! The analysis lives in [`crate::leftover_print_debug::domain`], which also backs the
//! standalone `inspect leftover-print-debug` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_print_debug::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

/// Report-only, deliberately, and this is the one fact about the rule most
/// likely to be "tidied" back to `Fixable` by someone reading only
/// [`crate::leftover_print_debug::domain`] — where a `fix_span` is still
/// computed and still correct. The span is not the problem; what it points at
/// is.
///
/// The removal-safety analysis in [`crate::support`] establishes that deleting
/// the call cannot change the *value* of its enclosing body. It cannot
/// establish that the call was a debug leftover rather than the program's
/// output, and in every dialect in `DIALECTS` the heads it matches are that
/// dialect's ordinary way to write to a stream: Scheme's `display`, Racket's
/// `displayln`, Emacs Lisp's `message`, Clojure's `println`, Janet's, Fennel's
/// and Hy's `print`, and — the case that looks most like a genuine debug
/// vocabulary and is not — Common Lisp's `princ`/`prin1`/`pprint`. Deleting
/// one of those is a semantic change no reader can see: the file still parses,
/// and the program stops speaking.
///
/// Measured over 8,959 SHA-256-deduplicated third-party files (9,189 raw)
/// across all eight dialects — sbcl, hunchentoot, clack, quicklisp-client,
/// babel, bordeaux-threads, asdf, emacs, magit, doomemacs, chibi-scheme,
/// guile, chez-srfi, racket, janet, spork, jpm, clojurescript, reitit, fennel,
/// conjure, hy — the rule produced 7,154 findings, 2,005 of them carrying a
/// fix. In a seeded random sample of 169 findings stratified over dialect and
/// over fixability, 164 were deliberate program output. In the *fixable*
/// stratum specifically, 85 of 86 were: seven of the eight dialects sampled at
/// 100% false-positive there, Emacs Lisp at 10 of 11.
///
/// What that costs in practice: running `fix apply --rule
/// leftover-print-debug` over the 417 Emacs Lisp files carrying a fixable
/// finding deleted 987 `(message …)` calls from 368 files — including
/// `(message "Saved new authentication information to %s" file)` in
/// `auth-source.el` — and Emacs's own reader accepted all 417 files
/// afterwards. Syntactic reparse guards cannot catch this class; 49 files were
/// refused by the existing guard, and the other 368 were silently gutted.
///
/// Two alternatives were considered and rejected:
///
/// - **Narrowing [`crate::leftover_print_debug::domain::heads_for`].** For
///   five dialects the offending head is the *only* head the rule models, so
///   "narrowing" is deleting the dialect — a `dialect_scope` change, not a fix
///   to this one. Nor do the numbers say which heads to keep: Janet's `pp` and
///   Clojure's `prn`, the two plausibly debug-only heads, drew 12 and 15
///   findings, too few to rate.
/// - **Keeping the fix for Common Lisp alone**, gating `fix_span` on the
///   dialect. Attractive because it moves no pinned count, but refuted by
///   measurement: all 14 sampled fixable Common Lisp findings were false
///   positives, among them the body of `uiop`'s own `println`, the
///   `sb-aclrepl` `:macroexpand` command, and `(print form src)` calls writing
///   generated Lisp to a file.
pub const META: RuleMeta = RuleMeta::new(
    "leftover-print-debug",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a bare debug-print call left in committed source",
    Fixability::ReportOnly,
);

/// Every dialect [`crate::leftover_print_debug::domain::heads_for`] models a
/// debug-print vocabulary for.
const DIALECTS: [Dialect; 8] = [
    Dialect::CommonLisp,
    Dialect::EmacsLisp,
    Dialect::Clojure,
    Dialect::Scheme,
    Dialect::Racket,
    Dialect::Janet,
    Dialect::Fennel,
    Dialect::Hy,
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // The removal-safety analysis needs to see a call's position relative
        // to its enclosing body and any surrounding quote, which is not a
        // per-node predicate — see `paredit-feature-lint-repl-debug::support`.
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let candidates = evaluated_candidates(context, view);
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context);
        examine(
            candidates,
            &scope,
            context.dialect(),
            context.path(),
            &mut items,
        );
        for item in items {
            // No `report_fixed` branch, deliberately: see `META`. `item`'s
            // `fix_span` stays computed because the standalone
            // `inspect leftover-print-debug` report still names the span a
            // human would delete — it just no longer becomes a rewrite the
            // tool applies on its own.
            sink.report(
                item.span,
                format!("{} is a leftover debug-print call", item.head),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::model::Fixability;
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use super::{DIALECTS, META, RULE};

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// Runs the real dispatch and returns `(finding messages, rewritten source)`.
    ///
    /// The rewrite is applied exactly as `fix apply` would: every fix the rule
    /// attached, spliced into the source. With no fixes attached, that is the
    /// identity — which is the property under test, expressed as the *source*
    /// rather than as `fix.is_none()`, so it still fails if a fix reappears
    /// through some other path.
    fn run(source: &str, dialect: Dialect) -> (Vec<String>, String) {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let outcomes = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("probe.lisp"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("dispatch");

        let mut messages = Vec::new();
        let mut edits = Vec::new();
        for outcome in outcomes {
            let (finding, fix) = outcome.into_parts();
            messages.push(finding.message);
            if let Some(fix) = fix {
                for replacement in fix.replacements() {
                    edits.push((replacement.span(), replacement.text().to_owned()));
                }
            }
        }
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start().get()));
        let mut rewritten = source.to_owned();
        for (span, text) in edits {
            rewritten.replace_range(span.start().get()..span.end().get(), &text);
        }
        (messages, rewritten)
    }

    #[test]
    fn the_rule_is_report_only() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
    }

    /// The regression this rule was demoted for: every one of these is that
    /// dialect's primary output primitive in a position the old removal-safety
    /// analysis called `Safe`, so every one of them used to be deleted.
    ///
    /// Asserted as the rewritten source, byte for byte, rather than as an
    /// absent fix — deleting a call and reinserting equivalent text would pass
    /// the weaker check.
    #[test]
    fn a_primary_output_primitive_is_reported_and_the_source_is_left_alone() {
        let cases = [
            // (dialect, source) — each print sits before a trailing body form,
            // which is exactly the `RemovalSafety::Safe` shape.
            (
                Dialect::Scheme,
                "(define (serve line out)\n  (display line out)\n  (flush-output-port out))",
            ),
            (
                Dialect::Racket,
                "(define (emit l o)\n  (displayln l o)\n  (flush-output o))",
            ),
            (
                Dialect::EmacsLisp,
                "(defun save-it (file)\n  (message \"Saved %s\" file)\n  nil)",
            ),
            (
                Dialect::Clojure,
                "(defn start []\n  (println \"server running\")\n  :ok)",
            ),
            (
                Dialect::Janet,
                "(defn amalg [v]\n  (print \"/* generated from \" v \" */\")\n  :ok)",
            ),
            (Dialect::Fennel, "(fn help [text]\n  (print text)\n  nil)"),
            (Dialect::Hy, "(defn greet []\n  (print \"hello\")\n  None)"),
        ];
        for (dialect, source) in cases {
            let (messages, rewritten) = run(source, dialect);
            assert_eq!(
                rewritten, source,
                "{dialect:?}: the rule rewrote source it must only report on"
            );
            assert_eq!(
                messages.len(),
                1,
                "{dialect:?}: the finding must survive the demotion"
            );
        }
    }

    /// Anti-over-suppression control, keyed on the finding itself: demoting
    /// fixability must not have silenced anything. Common Lisp is the dialect
    /// whose heads really are debug-only, and it must still report — without
    /// this, the assertions above would pass for a rule that reported nothing.
    #[test]
    fn the_findings_themselves_are_unchanged_by_the_demotion() {
        let (messages, rewritten) = run("(print x)\n(+ 1 2)", Dialect::CommonLisp);
        assert_eq!(messages, vec!["print is a leftover debug-print call"]);
        assert_eq!(rewritten, "(print x)\n(+ 1 2)");

        // And the shapes the rule never reported still go unreported, so the
        // demotion did not widen it either.
        let (quoted, _) = run("'(print x)", Dialect::CommonLisp);
        assert!(quoted.is_empty());
    }

    #[test]
    fn every_declared_dialect_still_reports_its_own_vocabulary() {
        use crate::leftover_print_debug::domain::heads_for;
        for dialect in DIALECTS {
            let heads = heads_for(dialect);
            assert!(!heads.is_empty(), "{dialect:?} declared but unmodelled");
            for head in heads {
                let source = format!("({head} x)\n(f)");
                let (messages, rewritten) = run(&source, dialect);
                assert_eq!(messages.len(), 1, "{dialect:?}/{head} stopped reporting");
                assert_eq!(rewritten, source, "{dialect:?}/{head} rewrote source");
            }
        }
    }
}
