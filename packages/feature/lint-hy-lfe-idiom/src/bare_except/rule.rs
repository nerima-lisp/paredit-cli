//! `hy-bare-except`: `(except [] …)`, which is Python's bare `except:` and
//! catches Ctrl-C.
//!
//! An empty binding list compiles to an `ExceptHandler` with no type, which
//! catches every `BaseException` — including `KeyboardInterrupt` and
//! `SystemExit`, neither of which is an error the program should handle.
//! Measured against Hy 1.3.1:
//!
//! ```text
//! (try (raise (KeyboardInterrupt)) (except []            …))  =>  caught
//! (try (raise (KeyboardInterrupt)) (except [e Exception] …))  =>  propagated
//! (try (raise (SystemExit 3))      (except []            …))  =>  caught
//! ```
//!
//! This is Python's own `pycodestyle` E722 ("do not use bare except"), which
//! `flake8` enables by default; the rule is that convention read through Hy's
//! surface syntax.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::bare_except::domain::{DIALECTS, is_catch_everything};
use crate::shared::{is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "hy-bare-except",
    RuleCategory::Conditions,
    Severity::Warning,
    "`(except [] …)` catches every BaseException, Ctrl-C and SystemExit included",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "An `except` clause with an empty binding list compiles to a Python `ExceptHandler` with \
         no type, which is the bare `except:` — it catches `BaseException`, so it swallows the \
         `KeyboardInterrupt` raised by Ctrl-C and the `SystemExit` raised by `sys.exit`. \
         Measured against Hy 1.3.1, `(except [] …)` catches both and `(except [e Exception] …)` \
         catches neither. Python's own `pycodestyle` reports the same shape as E722.",
    )
    .with_example(
        "(try\n  (risky)\n  (except []\n    (log.warning \"failed\")))",
        "(try\n  (risky)\n  (except [e Exception]\n    (log.warning \"failed: %s\" e)))",
    )
    .with_caveat(
        "`(except [ValueError] …)` names a type without binding a name and is not reported; only \
         a literally empty `[]` is.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("except")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if list_head(view) != Some("except") {
            return Ok(());
        }
        if !is_catch_everything(view) {
            return Ok(());
        }
        // Only now, with a finding otherwise ready: this reads the root view.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            view.span,
            "this except clause names no exception type, so it catches every BaseException — \
             KeyboardInterrupt and SystemExit included; name the exceptions to handle"
                .to_owned(),
        );
        Ok(())
    }
}
