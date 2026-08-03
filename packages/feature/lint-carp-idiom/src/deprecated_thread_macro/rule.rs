//! `carp-deprecated-thread-macro`: a threading macro Carp's own standard
//! library marks deprecated.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleFix, RuleMeta,
    Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::deprecated_thread_macro::domain::{self, examine};
use crate::support::node_context;

pub const META: RuleMeta = RuleMeta::new(
    "carp-deprecated-thread-macro",
    RuleCategory::Portability,
    Severity::Warning,
    "a threading macro Carp's standard library marks deprecated",
    Fixability::Fixable,
)
.with_explanation(
    RuleExplanation::new(
        "Carp's `core/ControlMacros.carp` declares `(deprecated => \"deprecated in favor of \
         `->`.\")` and the same for `==>` against `-->`. That declaration only writes metadata: \
         the compiler reads the `deprecated` key in `primitiveInfo` (the REPL's `(info …)` \
         command) and in the HTML doc renderer, and nowhere on any compilation path. So the \
         deprecated spelling builds cleanly with no diagnostic at all, which is why it outlives \
         the deprecation — Carp's own `core/Binary.carp` still uses `==>`. The replacement is a \
         rename rather than a rewrite: `=>` and `->` are both defined as \
         `(defmacro _ [:rest forms] (thread-first-internal forms))`, with identical bodies.",
    )
    .with_example(
        "(=> state (update-pos) (draw))",
        "(-> state (update-pos) (draw))",
    )
    .with_caveat(
        "The fix is withheld — the finding is still reported — when the same file defines its \
         own `->` or `-->`, because the rename is only meaning-preserving while the replacement \
         resolves to core's binding.",
    )
    .with_caveat(
        "A test that deliberately exercises the deprecated macro is reported like any other use. \
         Four of the nine findings in an audit of Carp's own tree are of that kind, all in \
         `test/produces-output/basics.carp`'s `threading` function, which prints and evaluates \
         `(=> …)` and `(==> …)` on purpose. They are correctly identified as deprecated uses but \
         are not defects to fix; suppress them at the file level.",
    ),
);

/// One head per entry of [`domain::DEPRECATIONS`], written out because
/// `NormalizedHead::new` is `const` and the table is not a `const` iterator.
///
/// `head_key` is verbatim for Carp — there is no case folding — so these must
/// match the source spelling byte for byte.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("=>"), NormalizedHead::new("==>")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        // Reads the domain's own table, so the head set and the dialect gate
        // cannot drift apart in a later edit.
        RuleDialectScope::new(&domain::DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(item) = examine(context.dialect(), view) else {
            return Ok(());
        };
        // Asked only once a finding exists, and asked *once* for both facts.
        // The dispatcher hands a rule quoted nodes like any other, so a
        // `(=> …)` inside a macro template reaches here — but this
        // materializes the whole document, so asking before `examine` would
        // charge every visited node for a walk that almost always answers
        // "no", and asking twice cost four orders of magnitude when measured.
        let context_at = node_context(context.tree(), view.span, item.deprecation.replacement);
        if context_at.is_data {
            return Ok(());
        }
        let message = format!(
            "{} is deprecated in favor of {} ({}); it still compiles because Carp's \
             deprecation metadata is only read by `(info …)` and the doc renderer",
            item.deprecation.head, item.deprecation.replacement, item.deprecation.citation
        );
        // A file that defines its own `->` would have the rename change
        // meaning rather than preserve it, so the finding stands but the fix
        // does not.
        if context_at.defines_asked_name {
            sink.report(item.span, message);
            return Ok(());
        }
        let fix = RuleFix::single(
            item.head_span,
            item.deprecation.replacement.to_owned(),
            format!(
                "Replace {} with {}",
                item.deprecation.head, item.deprecation.replacement
            ),
        );
        sink.report_fixed(item.span, message, fix);
        Ok(())
    }
}
