//! `malformed-cond-clause`: a cond clause that is not a non-empty list.
//!

use paredit_core_lint_engine::LintResult;

use crate::malformed_cond_clause::domain::examine_cond;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "malformed-cond-clause",
    RuleCategory::Malformed,
    Severity::Error,
    "a cond clause that is not a non-empty list",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cond")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(context.tree(), view, &mut cond_form_count, &mut items)?;
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!("cond clause {} is not a non-empty list", item.clause),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn findings(input: &str) -> usize {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        collect_lint_outcomes(
            catalog,
            &index,
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            RuleSelection::All,
        )
        .expect("lint pass")
        .len()
    }

    #[test]
    fn dispatch_ignores_defstruct_slot_name_and_options() {
        assert_eq!(
            findings("(defstruct ast-if (cond nil) (value nil :type (cond nil)))"),
            0,
        );
    }

    #[test]
    fn dispatch_checks_defstruct_slot_initform() {
        assert_eq!(
            findings("(defstruct ast-if (cond (cond nil) :type (cond nil)))"),
            1,
        );
    }
}
