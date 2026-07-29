//! Per-rule cost accounting for `--timings`.
//!
//! A single pass over the document runs 143 rules, which is exactly what makes
//! the suite fast and exactly what makes a slow rule invisible: the wall clock
//! of a lint run is one number, and nothing in it says which rule spent it.
//! Guessing from the source is unreliable — the expensive rules are usually the
//! ones that touch a semantic table, and that cost lands on whichever rule
//! happened to ask for the table first.
//!
//! So the dispatcher can be asked to time each `check` call and attribute it to
//! the rule that made it. The measurement is opt-in because it is not free: two
//! clock reads per (rule, node) pair is a real per-node cost that an untimed
//! run must not pay.
//!
//! First-use table construction is attributed honestly rather than amortized —
//! the rule that triggers the build is charged for it. That is the number a
//! reader wants: it says "asking for the binding table costs this much", and
//! the `invocations` column next to it says the rule was only called once, so
//! the cost is not per-node.

use std::time::Duration;

use super::ordering::RuleIndex;

/// Wall-clock time and call count per rule, indexed by registration position.
#[derive(Debug, Clone)]
pub struct RuleTimings {
    elapsed: Box<[Duration]>,
    invocations: Box<[u64]>,
}

impl RuleTimings {
    /// A zeroed table sized for `rule_count` rules.
    #[must_use]
    pub fn new(rule_count: usize) -> Self {
        Self {
            elapsed: vec![Duration::ZERO; rule_count].into_boxed_slice(),
            invocations: vec![0; rule_count].into_boxed_slice(),
        }
    }

    /// Records one `check` call.
    pub fn record(&mut self, rule: RuleIndex, elapsed: Duration) {
        let index = rule.get();
        self.elapsed[index] += elapsed;
        self.invocations[index] += 1;
    }

    /// Folds another file's measurements in, so a workspace run reports one
    /// table rather than one per file.
    ///
    /// # Panics
    ///
    /// If the two tables were sized for different rule counts, which can only
    /// happen if they came from different catalogues.
    pub fn merge(&mut self, other: &Self) {
        assert_eq!(
            self.elapsed.len(),
            other.elapsed.len(),
            "merging timing tables built for different rule counts"
        );
        for index in 0..self.elapsed.len() {
            self.elapsed[index] += other.elapsed[index];
            self.invocations[index] += other.invocations[index];
        }
    }

    #[must_use]
    pub fn elapsed_of(&self, rule: RuleIndex) -> Duration {
        self.elapsed[rule.get()]
    }

    #[must_use]
    pub fn invocations_of(&self, rule: RuleIndex) -> u64 {
        self.invocations[rule.get()]
    }

    /// The total across every rule, which is the part of the run this table
    /// accounts for (parsing and I/O are outside it).
    #[must_use]
    pub fn total(&self) -> Duration {
        self.elapsed.iter().sum()
    }

    /// Every rule's measurement in registration order, as
    /// `(position, elapsed, invocations)`. The caller pairs the position with
    /// its catalogue entry, so this module never names a rule.
    pub fn entries(&self) -> impl Iterator<Item = (usize, Duration, u64)> + use<'_> {
        self.elapsed
            .iter()
            .zip(self.invocations.iter())
            .enumerate()
            .map(|(index, (elapsed, invocations))| (index, *elapsed, *invocations))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_table_is_all_zeroes() {
        let timings = RuleTimings::new(3);
        assert_eq!(timings.total(), Duration::ZERO);
        assert_eq!(timings.invocations_of(RuleIndex::new(2)), 0);
        assert_eq!(timings.entries().count(), 3);
    }

    #[test]
    fn repeated_calls_accumulate_on_their_own_rule() {
        let mut timings = RuleTimings::new(2);
        timings.record(RuleIndex::new(0), Duration::from_micros(10));
        timings.record(RuleIndex::new(0), Duration::from_micros(5));
        timings.record(RuleIndex::new(1), Duration::from_micros(2));

        assert_eq!(
            timings.elapsed_of(RuleIndex::new(0)),
            Duration::from_micros(15)
        );
        assert_eq!(timings.invocations_of(RuleIndex::new(0)), 2);
        assert_eq!(
            timings.elapsed_of(RuleIndex::new(1)),
            Duration::from_micros(2)
        );
        assert_eq!(timings.total(), Duration::from_micros(17));
    }

    #[test]
    fn merging_sums_both_tables_rule_by_rule() {
        let mut first = RuleTimings::new(2);
        first.record(RuleIndex::new(0), Duration::from_micros(10));
        let mut second = RuleTimings::new(2);
        second.record(RuleIndex::new(0), Duration::from_micros(1));
        second.record(RuleIndex::new(1), Duration::from_micros(4));

        first.merge(&second);
        assert_eq!(
            first.elapsed_of(RuleIndex::new(0)),
            Duration::from_micros(11)
        );
        assert_eq!(first.invocations_of(RuleIndex::new(0)), 2);
        assert_eq!(first.invocations_of(RuleIndex::new(1)), 1);
    }

    #[test]
    #[should_panic(expected = "different rule counts")]
    fn merging_mismatched_tables_is_a_bug_not_a_silent_truncation() {
        let mut first = RuleTimings::new(2);
        first.merge(&RuleTimings::new(3));
    }

    #[test]
    fn entries_come_back_in_registration_order() {
        let mut timings = RuleTimings::new(3);
        timings.record(RuleIndex::new(2), Duration::from_micros(9));
        let positions: Vec<usize> = timings.entries().map(|(index, _, _)| index).collect();
        assert_eq!(positions, vec![0, 1, 2]);
    }
}
