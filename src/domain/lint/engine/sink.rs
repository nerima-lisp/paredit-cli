//! Where a rule puts what it finds.

use std::path::{Path, PathBuf};

use crate::domain::sexpr::ByteSpan;

use super::ordering::{OutcomeOrder, RuleIndex, VisitIndex};
use crate::domain::lint::model::{LintFinding, LintOutcome, RuleFix, RuleName};

/// Collects every rule's findings during the single pass and hands them back in
/// the historical order.
///
/// A rule never sees this type directly: [`FindingSink::visiting`] lends it a
/// [`RuleSink`] already bound to that rule's identity and to the node being
/// visited, so a rule cannot mislabel a finding as another rule's or place it
/// at the wrong point in the walk.
#[derive(Debug)]
pub struct FindingSink<'a> {
    path: &'a Path,
    entries: Vec<(OutcomeOrder, LintOutcome)>,
    next_emission: u32,
}

impl<'a> FindingSink<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self {
            path,
            entries: Vec::new(),
            next_emission: 0,
        }
    }

    /// Lends a rule-and-node-scoped view of this sink for the duration of one
    /// `check` call.
    pub fn visiting(
        &mut self,
        rule: RuleIndex,
        name: RuleName,
        visit: VisitIndex,
    ) -> RuleSink<'_, 'a> {
        RuleSink {
            sink: self,
            rule,
            name,
            visit,
        }
    }

    /// Every outcome, sorted back into "registry order, then each rule's own
    /// pre-order".
    pub fn into_ordered(mut self) -> Vec<LintOutcome> {
        self.entries.sort_by_key(|(order, _)| *order);
        self.entries.into_iter().map(|(_, outcome)| outcome).collect()
    }

    fn push(
        &mut self,
        order_key: (RuleIndex, VisitIndex),
        rule: RuleName,
        span: ByteSpan,
        message: String,
        fix: Option<RuleFix>,
    ) {
        let order = OutcomeOrder::new(order_key.0, order_key.1, self.next_emission);
        self.next_emission += 1;
        let finding = LintFinding {
            rule: rule.as_str(),
            path: PathBuf::from(self.path),
            span,
            message,
        };
        self.entries.push((order, LintOutcome::new(finding, fix)));
    }
}

/// The sink as one rule sees it while checking one node.
#[derive(Debug)]
pub struct RuleSink<'sink, 'a> {
    sink: &'sink mut FindingSink<'a>,
    rule: RuleIndex,
    name: RuleName,
    visit: VisitIndex,
}

impl RuleSink<'_, '_> {
    /// Reports a diagnostic-only finding.
    pub fn report(&mut self, span: ByteSpan, message: impl Into<String>) {
        self.sink.push(
            (self.rule, self.visit),
            self.name,
            span,
            message.into(),
            None,
        );
    }

    /// Reports a finding together with the rewrite that repairs it.
    ///
    /// `span` is the finding's own span and doubles as the fix's identity key;
    /// the fix's replacements may edit a narrower region (substituting just the
    /// head operator, say) or several disjoint ones.
    pub fn report_fixed(&mut self, span: ByteSpan, message: impl Into<String>, fix: RuleFix) {
        self.sink.push(
            (self.rule, self.visit),
            self.name,
            span,
            message.into(),
            Some(fix),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sexpr::ByteOffset;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    #[test]
    fn interleaved_emissions_come_back_in_registry_then_walk_order() {
        let path = Path::new("test.lisp");
        let mut sink = FindingSink::new(path);
        let late = RuleName::new("late-rule");
        let early = RuleName::new("early-rule");

        // A later-registered rule fires on the first node, an earlier one on
        // the second: exactly the interleaving one pass produces.
        sink.visiting(RuleIndex::new(5), late, VisitIndex::new(1))
            .report(span(0, 1), "late at node 1");
        sink.visiting(RuleIndex::new(2), early, VisitIndex::new(2))
            .report(span(2, 3), "early at node 2");
        sink.visiting(RuleIndex::new(2), early, VisitIndex::new(1))
            .report(span(0, 1), "early at node 1");

        let messages: Vec<String> = sink
            .into_ordered()
            .into_iter()
            .map(|outcome| outcome.finding().message.clone())
            .collect();
        assert_eq!(
            messages,
            vec!["early at node 1", "early at node 2", "late at node 1"]
        );
    }

    #[test]
    fn repeated_findings_on_one_node_keep_their_emission_order() {
        let path = Path::new("test.lisp");
        let mut sink = FindingSink::new(path);
        let rule = RuleName::new("duplicate-case-key");
        {
            let mut scoped = sink.visiting(RuleIndex::new(0), rule, VisitIndex::new(4));
            scoped.report(span(10, 20), "first duplicate");
            scoped.report(span(10, 20), "second duplicate");
        }
        let messages: Vec<String> = sink
            .into_ordered()
            .into_iter()
            .map(|outcome| outcome.finding().message.clone())
            .collect();
        assert_eq!(messages, vec!["first duplicate", "second duplicate"]);
    }

    #[test]
    fn a_reported_fix_travels_with_its_finding() {
        let path = Path::new("test.lisp");
        let mut sink = FindingSink::new(path);
        sink.visiting(RuleIndex::new(0), RuleName::new("redundant-quote"), VisitIndex::new(1))
            .report_fixed(
                span(0, 2),
                "quote is redundant",
                RuleFix::single(span(0, 2), "x", "Remove the redundant quote"),
            );
        let outcomes = sink.into_ordered();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            outcomes[0].fix().expect("fix").description(),
            "Remove the redundant quote"
        );
    }
}
