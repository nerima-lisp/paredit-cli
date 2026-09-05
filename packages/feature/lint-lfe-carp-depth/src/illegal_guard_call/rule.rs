//! `lfe-illegal-guard-call`: a module-qualified call in a `when` guard that
//! Erlang's guard sublanguage does not permit.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::illegal_guard_call::domain::{self, Reason, collect_in_guard};
use crate::support::node_context;

pub const META: RuleMeta = RuleMeta::new(
    "lfe-illegal-guard-call",
    RuleCategory::Malformed,
    Severity::Error,
    "a module-qualified call in a `when` guard that Erlang does not permit",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "LFE inherits Erlang's guard restriction: a guard may only call a fixed set of BIFs and \
         operators. `lfe_lint.erl` rejects anything else with `illegal guard expression`, and \
         its `check_gexpr/4` permits a qualified call in exactly one case — module `erlang`, a \
         function in the guard BIF set, at an arity that set allows. Every other \
         module-qualified call in a guard is a compile error, not a style question. Verified \
         against LFE 2.2.0 on OTP 27.3.4.15: `(lists:member x '(1 2))` and \
         `(call 'lists 'member x '(1 2))` in a guard both fail to compile, while \
         `(erlang:is_atom x)` compiles cleanly.",
    )
    .with_example(
        "(defun f ((x) (when (lists:member x '(1 2))) 'yes) ((_) 'no))",
        "(defun f ((x) (when (erlang:is_list x)) 'yes) ((_) 'no))",
    )
    .with_caveat(
        "Only module-qualified calls are reported. An unqualified call to a user function in a \
         guard is also a compile error, but it cannot be distinguished from a macro call without \
         an environment, and LFE expands macros before linting — `clj.lfe` defines `atom?` as a \
         macro over `is_atom` and uses it in its own guards. `binding_table()` is empty for LFE, \
         so reporting unqualified heads would false-positive on every such macro.",
    )
    .with_caveat(
        "The arity used for the verdict is the one written at the call site. A guard BIF name at \
         an arity Erlang does not permit is reported as a wrong-arity finding rather than an \
         unknown function, because `is_record/1` is a different defect from `list_to_atom/1`.",
    ),
);

/// The three heads a module-qualified call can have.
///
/// `head_key` is verbatim for LFE — no case folding — so these match the
/// source spelling byte for byte.
///
/// Only `when` is here, not `call` or `:`. The rule is about calls *in a
/// guard*, and the guard is what gives them their meaning: `(lists:member x
/// y)` is perfectly legal in a function body. Anchoring on `when` means the
/// dispatcher hands over exactly the forms that can contain a finding, and the
/// rule walks that small subtree itself. Anchoring on `call` instead would
/// visit every remote call in the file and then have to walk *up* to find out
/// whether it sat in a guard, which the view does not support.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("when")];

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
        // Cheap first: walks only the guard's own subtree, and answers "no"
        // for every guard that calls nothing qualified — which is nearly all
        // of them.
        let found = collect_in_guard(context.dialect(), view);
        if found.is_empty() {
            return Ok(());
        }
        // Only now, with findings otherwise ready, the single root-view
        // descent. It materializes the whole document, so asking before
        // `collect_in_guard` would charge every visited `when` for a walk that
        // almost always answers "no" — the ordering mistake that measured
        // 450843 ns/call against 28 ns/call in the reference measurement.
        //
        // A `(when …)` inside a quoted list, a `` ` `` template or a
        // `defsyntax` rule is not a guard the compiler will ever check; see
        // the rule's caveat. The quasiquote case is not hypothetical — the
        // corpus audit's only third-party guard finding was
        // `(when ,(lanes.util:not-in 'method methods))` inside a template,
        // where the qualified call runs at expansion time to *produce* the
        // guard rather than inside it.
        if node_context(context.tree(), view.span).suppresses_findings() {
            return Ok(());
        }
        for item in found {
            let detail = match item.reason {
                Reason::NotErlangModule => format!(
                    "only `erlang` may be called, qualified, from a guard, and this calls \
                     `{}`",
                    item.module
                ),
                Reason::NotAGuardBif => format!(
                    "`erlang:{}/{}` is not one of the BIFs Erlang permits in a guard",
                    item.function, item.arity
                ),
                Reason::WrongArity => format!(
                    "`erlang:{}` is a guard BIF, but not at arity {}",
                    item.function, item.arity
                ),
                Reason::NotLiteral => "the module and function must be literal atoms, and a \
                                       guard may only call `erlang`"
                    .to_owned(),
            };
            sink.report(
                item.span,
                format!(
                    "illegal guard expression: {detail}; LFE rejects this with `illegal guard \
                     expression` (spelled here as `{}`)",
                    item.syntax.describe()
                ),
            );
        }
        Ok(())
    }
}

/// Kept next to the rule so a new [`CallSyntax`] cannot be added without a
/// message for it.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::illegal_guard_call::domain::CallSyntax;

    #[test]
    fn every_call_syntax_describes_itself() {
        for syntax in [
            CallSyntax::CallForm,
            CallSyntax::Colon,
            CallSyntax::ColonForm,
        ] {
            assert!(!syntax.describe().is_empty());
        }
    }

    #[test]
    fn the_rule_is_scoped_to_lfe_only() {
        assert_eq!(domain::DIALECTS.len(), 1);
    }
}
