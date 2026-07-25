//! Reconstructing the historical finding order from a single pass.
//!
//! Before the single-pass dispatcher, each rule walked the whole document on
//! its own and pushed findings in registry order, so the report read
//! "rule order, then each rule's own pre-order". One pass emits findings
//! interleaved instead, and the order has to be restored exactly — an
//! `inspect lint` report is a published, byte-compared contract.
//!
//! Sorting by `(rule, span)` looks sufficient but is not: rules that flag
//! several problems in one form (the duplicate-key family) emit repeated
//! findings on the *same* span, and the tie has no defined resolution.
//! Counting instead — which rule, which node of the walk, which emission
//! within that node — is a total order by construction and never consults a
//! span at all.

/// A rule's position in the registry, which is also its output rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleIndex(u16);

impl RuleIndex {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// A node's position in the dispatcher's pre-order walk.
///
/// Pre-order is what the old per-rule walk
/// ([`crate::domain::view_query::for_each_subview`], applied to each root child
/// in turn) produced, so matching it node-for-node is what reproduces each
/// rule's internal order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VisitIndex(u32);

impl VisitIndex {
    pub const ROOT: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Where one finding sits in the reconstructed order. Field order is the sort
/// order; `Ord` is derived so the two cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OutcomeOrder {
    rule: RuleIndex,
    visit: VisitIndex,
    emission: u32,
}

impl OutcomeOrder {
    pub const fn new(rule: RuleIndex, visit: VisitIndex, emission: u32) -> Self {
        Self {
            rule,
            visit,
            emission,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(rule: u16, visit: u32, emission: u32) -> OutcomeOrder {
        OutcomeOrder::new(RuleIndex::new(rule), VisitIndex::new(visit), emission)
    }

    #[test]
    fn rule_rank_dominates_position_in_the_walk() {
        assert!(order(0, 900, 0) < order(1, 1, 0));
    }

    #[test]
    fn within_a_rule_the_walk_decides() {
        assert!(order(3, 1, 0) < order(3, 2, 0));
    }

    #[test]
    fn repeated_findings_on_one_node_keep_emission_order() {
        assert!(order(3, 7, 0) < order(3, 7, 1));
    }
}
