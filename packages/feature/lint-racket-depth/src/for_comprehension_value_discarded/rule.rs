//! `racket-for-comprehension-value-discarded`: a container-building `for/`
//! comprehension in a statement position.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::for_comprehension_value_discarded::domain::{DIALECTS, examine_body_form};

pub const META: RuleMeta = RuleMeta::new(
    "racket-for-comprehension-value-discarded",
    RuleCategory::Allocation,
    Severity::Warning,
    "a for/list-family comprehension whose container is built and then dropped",
    Fixability::ReportOnly,
);

/// The *body forms*, not the comprehensions. See the domain module for why the
/// rule is anchored this way round.
const FILTER_HEADS: [NormalizedHead; 11] = [
    NormalizedHead::new("begin"),
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("\u{3bb}"),
    NormalizedHead::new("define"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("letrec"),
    NormalizedHead::new("letrec*"),
    NormalizedHead::new("parameterize"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&FILTER_HEADS)
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
        let mut body_form_count = 0;
        let mut items = Vec::new();
        examine_body_form(context.tree(), view, &mut body_form_count, &mut items);
        for item in items {
            sink.report(item.span, paredit_core_cli::report::Finding::message(&item));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::for_comprehension_value_discarded::domain::HEADS;

    /// An under-approximating `HeadFilter` silently drops every finding at a
    /// head the domain can match; pinning the two together makes adding a head
    /// to one and not the other a test failure rather than a silent hole.
    #[test]
    fn the_filter_heads_are_exactly_the_domain_heads() {
        let filter: Vec<&str> = FILTER_HEADS.iter().map(|head| head.as_str()).collect();
        assert_eq!(filter, HEADS.to_vec());
    }

    #[test]
    fn every_domain_head_is_index_normalized() {
        for head in HEADS {
            assert!(!head.is_empty());
            assert!(!head.contains(':'), "{head} would never reach the index");
            assert_eq!(head, head.to_ascii_lowercase(), "{head} is not normalized");
        }
    }
}
