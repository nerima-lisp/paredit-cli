//! `leftover-format-debug-marker`: a (format t ...) whose control string carries a DEBUG/DBG marker.
//!
//! The analysis lives in [`crate::leftover_format_debug_marker::domain`], which also backs the
//! standalone `inspect leftover-format-debug-marker` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_format_debug_marker::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// Report-only, deliberately, and for a reason distinct from its siblings: this
/// rule's marker test is wrong, not just its safety argument.
///
/// [`super::domain::contains_debug_marker`] requires a word boundary *before*
/// the marker and none after it. Its own doc gives `"UNDEBUGGABLE"` as the
/// non-match, which holds only because the `U` precedes `DEBUG`. Nothing stops
/// `DEBUG` from matching when it is the *start* of an ordinary word, so
/// `"debugger"`, `"debug-vregs"`, `"DEBUG_ASDF_TEST"` and `"debug font"` all
/// match, as does `"*check-built-in-constants-debug*"`.
///
/// Measured over 31,634 SHA-256-deduplicated third-party files (1.108 GB) the
/// rule produced 6 findings across 5 files, *every one carrying a fix*, deleting
/// 1,022 bytes. All 6 were adjudicated as a **census — 6 of 6 were false
/// positives, 100%**, and not one of them was a debug marker:
///
/// - ACL2's `interface-raw.lisp` loses a **638-byte user-facing error report**
///   (`"~%ERROR: Failed check for coverage of functions with acl2-loop-only code
///   differences!  Please send this error message to the ACL2 implementors…"`)
///   because the message mentions the variable `*check-built-in-constants-debug*`.
/// - FiveAM's `test.lisp` loses `"~&Interactive mode (DEBUG_ASDF_TEST) --
///   Invoke debugger.~%"`, matched on an *environment variable name*.
/// - Mezzano's debugger loses two lines of its own `:help` output — including
///   `"~&You are in the debugger. Commiserations!~%"` — matched on the word
///   "debugger" in prose, leaving a help listing with a hole in it.
/// - Mezzano's register allocator loses exactly one line,
///   `(format t "debug-vregs: ~:S~%" debug-vregs)`, out of eight sibling
///   diagnostic prints guarded by `(when (not ir::*shut-up*) …)`, matched on a
///   *variable name*.
/// - Mezzano's cold generator loses `";; Saving 8x8 debug font.~%"`, a build
///   progress line, because the font is called the debug font.
///
/// Narrowing was measured and rejected. Requiring a *trailing* boundary as well
/// — the obvious repair — removes only 2 of the 6: `"debug-vregs"`,
/// `"DEBUG_ASDF_TEST"`, `"debug font"` and `"*…-debug*"` are each followed by a
/// non-alphanumeric character and survive it. That leaves 4 false positives and
/// still no true positive anywhere in the corpus, so narrowing does not produce
/// a rule worth keeping fixable.
pub const META: RuleMeta = RuleMeta::new(
    "leftover-format-debug-marker",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (format t ...) whose control string carries a DEBUG/DBG marker",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
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
        examine(candidates, &scope, context.path(), &mut items);
        for item in items {
            // No `report_fixed` branch, deliberately: see `META`. `item`'s
            // `fix_span` stays computed because the standalone
            // `inspect leftover-format-debug-marker` report still names the span
            // a human would delete — it just no longer becomes a rewrite the
            // tool applies on its own.
            sink.report(
                item.span,
                "format's control string carries a DEBUG/DBG marker".to_owned(),
            );
        }
        Ok(())
    }
}
