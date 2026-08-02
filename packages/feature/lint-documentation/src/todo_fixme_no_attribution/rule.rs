//! `todo-fixme-no-attribution`: a task marker with no owner, ticket, or date.
//!
//! The analysis lives in [`crate::todo_fixme_no_attribution::domain`]; this
//! module declares the rule's metadata and its head filter.
//!
//! # Why this is `WholeTree` and not `Heads`
//!
//! Every other rule in this package is `Heads`, and should be. This one cannot
//! be, for a structural reason rather than a stylistic one: **its subject is
//! not a node.** Comments are trivia the parser keeps in a list beside the
//! tree, so there is no head — no node at all — for a head filter to select.
//! `HeadFilter::Heads` would name operators this rule does not care about and
//! would still have to read the whole comment list on each of them;
//! `HeadFilter::AllNodes` would read it once per node. Neither is a filter,
//! and both are worse than the honest declaration.
//!
//! This is the same reasoning `defpackage-without-in-package` and
//! `commented-out-code` document, and it is free rather than merely cheaper:
//! `collect_lint_pass` materializes the root view unconditionally, before the
//! `whole_tree()` loop, and hands that same view to every `WholeTree` rule. The
//! `view` argument this rule is given is that root — it is not rebuilt, and
//! this rule never calls `tree.root_view()` itself.
//!
//! # The cost of being dispatched on every file
//!
//! `WholeTree` means `check` runs once on **every file**, so the rejection path
//! for a file with nothing to say has to be genuinely cheap. It is, and without
//! needing a byte-scan guard, because the comment list *is* the guard:
//!
//! 1. `tree.comments()` on a file with no comments is an empty iterator. It
//!    allocates nothing and visits nothing — cheaper than the byte scan a
//!    guard would add. The `clean/forms/*` benchmarks are exactly this shape.
//! 2. A file that *does* have comments pays one prefix comparison of at most
//!    five bytes per comment, before anything is allocated. A comment that is
//!    not a marker stops there.
//! 3. Only a confirmed marker's tail is scanned for attribution.
//! 4. Never `binding_table()`/`value_table()`/`type_table()`; this rule needs
//!    no semantic pass and asks for none. Never `scratch_cache` either.
//!
//! Nothing here is quadratic. The file's comments are read once per `check`,
//! and `check` runs once per file — not once per marker, which is the shape
//! that turns N findings into N² work.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, RuleTag, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::todo_fixme_no_attribution::domain::collect;

pub const META: RuleMeta = RuleMeta::new(
    "todo-fixme-no-attribution",
    // Descriptive metadata a comment should carry and does not. The marker is
    // documentation of intended work; without an owner it documents nothing
    // actionable.
    RuleCategory::Documentation,
    Severity::Warning,
    "a TODO/FIXME marker with no owner, ticket reference, or date",
    // Who owns a task is not something a rewrite can infer, and this project
    // has already shipped one write command that silently deleted every
    // comment in a file — comment edits get no autofix here.
    Fixability::ReportOnly,
)
// Whether every marker needs an owner is a project's decision, and on a project
// that has decided otherwise this fires on all of them.
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "An unattributed marker is a note to nobody: no owner to ask, no ticket to schedule, and \
         no date to judge its age by. Finding out costs a `git blame`. One more token answers all \
         three questions.",
    )
    .with_example(
        ";; TODO: handle the empty case",
        ";; TODO(ada): handle the empty case (#412)",
    )
    .with_caveat(
        "Acceptance is deliberately wide: an owner in parentheses or brackets, an `@handle`, a \
         `#412`, any `PREFIX-412` tracker key, a URL, or a date in either ISO or slashed form all \
         count. A marker referenced in a shape this list does not name is a false positive, which \
         is why the list errs long.",
    ),
);

/// Every dialect this tool parses, because every one of them writes a comment
/// with `;` and none of them makes a `TODO` mean something else.
///
/// The sibling rules in this package are Common Lisp only, because a docstring
/// position is a language's own grammar. A comment is not.
const DIALECTS: [Dialect; 11] = Dialect::ALL;

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // `_view` is the root the dispatcher already built, and is deliberately
        // unused: this rule's subject is the comment list, which is not in it.
        // Reading the tree here rather than rebuilding a view is the point.
        for item in collect(context.tree()) {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::{run_rule_in, run_rule_with};
    use paredit_core_lint_engine::model::RuleSettings;
    use paredit_core_lint_engine::rule::RuleEntry;

    /// A one-rule catalogue, so the engine's own dispatch decides whether and
    /// how often `check` is called. Under `WholeTree` no head index is
    /// consulted, so what this pins is the dispatch mode itself: a wrong
    /// `HeadFilter` here means the rule runs the wrong number of times.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule_with(&ENTRIES, source, &RuleSettings::new())
    }

    #[test]
    fn a_bare_marker_fires_through_the_real_dispatch() {
        assert_eq!(
            messages(";; TODO: handle the empty case\n(defun f () 1)\n").len(),
            1
        );
    }

    /// The `WholeTree` property that matters: `check` runs once per *file*, so
    /// three markers in one file are three findings from one call, not three
    /// calls' worth of repeated whole-file work.
    #[test]
    fn three_markers_in_one_file_produce_three_findings() {
        let found = messages(";; TODO: a\n(defun f () 1)\n;; FIXME: b\n;; XXX: c\n");
        assert_eq!(found.len(), 3, "{found:?}");
    }

    /// The regression `WholeTree` could introduce: `check` runs on files that a
    /// head filter would have excluded, so a file with nothing to say must be
    /// settled by the empty comment list.
    #[test]
    fn a_file_with_no_comments_produces_nothing() {
        assert!(messages("(defun f () 1)\n(defvar *x* 2)\n").is_empty());
        assert!(messages("").is_empty());
    }

    #[test]
    fn an_attributed_marker_produces_nothing_through_the_real_dispatch() {
        assert!(messages(";; TODO(ada): rewrite this\n(defun f () 1)\n").is_empty());
        assert!(messages(";; TODO: drop once #412 lands\n(defun f () 1)\n").is_empty());
    }

    /// The comment-rule analogue of the quote guard: a marker inside a string
    /// literal is not a comment.
    #[test]
    fn a_marker_inside_a_string_literal_does_not_fire() {
        assert!(messages("(defun f () \"; TODO: not a comment\")").is_empty());
        assert!(messages("(format nil \";; FIXME: later~%\")").is_empty());
    }

    /// A quoted *form* cannot hide a comment either — but for the opposite
    /// reason to the node rules: the comment is beside the tree, so quoting the
    /// form it sits next to changes nothing about it. Pinned so that a future
    /// reader does not "fix" this rule by adding an `is_unevaluated_at` call
    /// that would silence real markers.
    #[test]
    fn a_marker_beside_quoted_data_still_fires() {
        assert_eq!(messages(";; TODO: later\n'(defun f () 1)\n").len(), 1);
    }

    /// The rule is declared for every dialect, and the dispatcher checks that
    /// before walking anything.
    ///
    /// Each dialect is asked in *its own* comment syntax. Janet is the one that
    /// does not write a comment with `;` — `;` is its splice operator, and its
    /// line comment is `#` (`ReaderPolicy::line_comment_width`). Asking every
    /// dialect with `;` would have read Janet's answer as "the rule does not
    /// run here" when what it actually means is "that was not a comment".
    #[test]
    fn the_rule_runs_in_every_dialect_in_that_dialects_comment_syntax() {
        for dialect in Dialect::ALL {
            let lead = if dialect == Dialect::Janet { "#" } else { ";;" };
            let found = run_rule_in(
                &ENTRIES,
                &format!("{lead} TODO: handle the empty case\n(f)\n"),
                &RuleSettings::new(),
                dialect,
            );
            assert_eq!(found.len(), 1, "no finding in {dialect:?}");
        }
    }

    /// The other half: an attributed marker is silent in every dialect too, so
    /// the test above is not passing because the rule fires unconditionally.
    #[test]
    fn an_attributed_marker_is_silent_in_every_dialect() {
        for dialect in Dialect::ALL {
            let lead = if dialect == Dialect::Janet { "#" } else { ";;" };
            let found = run_rule_in(
                &ENTRIES,
                &format!("{lead} TODO(ada): handle the empty case\n(f)\n"),
                &RuleSettings::new(),
                dialect,
            );
            assert!(found.is_empty(), "fired in {dialect:?}: {found:?}");
        }
    }

    #[test]
    fn a_clojure_and_a_scheme_file_are_read_the_same_way() {
        assert_eq!(
            run_rule_in(
                &ENTRIES,
                ";; TODO: later\n(defn f [] 1)\n",
                &RuleSettings::new(),
                Dialect::Clojure,
            )
            .len(),
            1
        );
        assert!(
            run_rule_in(
                &ENTRIES,
                ";; TODO(ada): later\n(define (f) 1)\n",
                &RuleSettings::new(),
                Dialect::Scheme,
            )
            .is_empty()
        );
    }
}
