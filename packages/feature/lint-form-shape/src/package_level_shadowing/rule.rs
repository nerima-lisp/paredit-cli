//! `package-level-shadowing`: an inner `let`/`let*` binding or a
//! `defun`/`defmacro`'s own lambda-list parameter that reuses the name of a
//! top-level `defun`/`defvar`/`defparameter`/`defconstant`/`defmacro` in the
//! same file.
//!
//! The analysis lives in [`crate::package_level_shadowing::domain`], which
//! also backs the standalone `inspect package-level-shadowing` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::package_level_shadowing::domain::{examine, top_level_names};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::view_query::for_each_subview;

pub const META: RuleMeta = RuleMeta::new(
    "package-level-shadowing",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an inner let binding or lambda-list parameter that reuses the name of a top-level \
     definition in the same file",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A local named the same as a top-level function or special variable makes the outer one \
         unreachable for the rest of that binding's scope. `inspect shadowed-bindings` already \
         covers shadowing between nested lexical scopes; this is the wider case, an inner binding \
         reaching all the way out to a top-level name.",
    )
    .with_example(
        "(defparameter *limit* 10)\n(defun check (x) (let ((*limit* 5)) (< x *limit*)))",
        "(defparameter *limit* 10)\n(defun check (x) (let ((local-limit* 5)) (< x local-limit*)))",
    )
    .with_caveat(
        "Only let/let* bindings and a defun/defmacro's own lambda-list parameters are read, the \
         same two sources `inspect shadowed-bindings` tracks. Lambda-list parsing is shallow: \
         nested destructuring lists a macro lambda list can have are not read.",
    ),
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Judging "does this name also name a top-level definition" needs
        // every top-level definition in the file at once.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let top_level = top_level_names(view);
        let mut scanned_form_count = 0;
        let mut items = Vec::new();
        for form in &view.children {
            for_each_subview(form, |subview| {
                examine(
                    subview,
                    context.path(),
                    &top_level,
                    &mut scanned_form_count,
                    &mut items,
                );
            });
        }
        for item in items {
            sink.report(
                item.span,
                format!(
                    "this {} named {} shadows a top-level definition of the same name",
                    item.source, item.name
                ),
            );
        }
        Ok(())
    }
}
