//! `symbol-function-fset-dynamic-name`: a function definition installed under a
//! name built by `intern` at run time.
//!

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::symbol_function_fset_dynamic_name::domain::examine;

pub const META: RuleMeta = RuleMeta::new(
    "symbol-function-fset-dynamic-name",
    // The category's own definition is "untrusted input reaching eval, read,
    // `intern`, or a subprocess". This is `intern`'s output installed as a
    // *function definition*, which is that path at its sharpest: whatever
    // computes the string decides what gets defined.
    RuleCategory::Security,
    Severity::Warning,
    "a function definition installed under a name built by intern, which no search can connect to \
     its callers",
    // No fix. Replacing a computed definition with named ones, or with a
    // dispatch table, changes the program's structure; a rewrite cannot pick
    // between them.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A function whose name exists only at run time is invisible to grep, to a cross-reference \
         index, and to `who-calls`. Whatever computes the name also decides which function cell \
         is overwritten, so a name derived from outside the program can replace an existing \
         definition.",
    )
    .with_example(
        "(setf (symbol-function (intern (format nil \"~A-handler\" kind))) #'run)",
        "(setf (gethash kind *handlers*) #'run)",
    )
    .with_caveat(
        "Only a name built by `intern`/`intern-soft` from something other than a string literal \
         is reported. `(setf (symbol-function 'foo) #'bar)` and `(fset (intern \"constant\") #'f)` \
         both spell their name in the source and are never reported.",
    ),
);

/// The heads `examine` can match: the two Emacs Lisp installers, plus `setf`
/// for the `(symbol-function …)` / `(fdefinition …)` places.
///
/// The union over dialects; `domain`'s per-dialect tables then reject the
/// pairings that do not exist, so a wider index costs a head comparison and
/// never a finding.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("defalias"),
    NormalizedHead::new("fset"),
    NormalizedHead::new("setf"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&[Dialect::CommonLisp, Dialect::EmacsLisp])
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        for item in examine(context.tree(), view, context.dialect()) {
            sink.report(
                item.span,
                format!(
                    "{} defines a function whose name {} builds at run time, so no search \
                     connects this definition to its callers",
                    item.installer, item.name_builder
                ),
            );
        }
        Ok(())
    }
}
