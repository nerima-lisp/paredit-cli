//! A wall-clock budget, checked between units of work.
//!
//! A timeout in an analysis tool is a correctness feature disguised as a
//! convenience one. Without it, one pathological file in a 50,000-file
//! traversal turns a CI job into a hang, and a hang reports nothing: the
//! 49,999 files that were fine produce no output either. With it, the run
//! fails with a named scope and a partial result.
//!
//! Two properties matter more than precision:
//!
//! - **Unarmed by default.** [`Deadline::unbounded`] never expires and never
//!   reads the clock. The determinism contract — same input, same bytes out —
//!   survives because nothing consults wall time unless a caller asked for a
//!   budget.
//! - **Cooperative, not pre-emptive.** The budget is checked at boundaries the
//!   caller names, so a check can say *which* file or rule was in flight. This
//!   package deliberately does not interrupt work in progress: killing a
//!   half-finished rewrite is precisely the outcome the rest of this crate
//!   exists to prevent.

use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use thiserror::Error;

/// The budget this process started with.
///
/// Global for the same reason the resource limits are: a deadline threaded by
/// hand through 130 command workflows is a deadline that some new command will
/// not receive, and a missing deadline is silently unbounded. Installed once
/// from the composition root before any command runs.
static PROCESS_DEADLINE: OnceLock<Deadline> = OnceLock::new();

/// Publishes the budget for this process. The first call wins.
pub fn install(deadline: Deadline) -> Result<(), Deadline> {
    PROCESS_DEADLINE.set(deadline).map_err(|_| effective())
}

/// The budget in force, or an unbounded one when nothing was installed.
///
/// Library callers and tests that never call [`install`] see no clock reads at
/// all, which is what keeps the same-input-same-output contract intact.
#[must_use]
pub fn effective() -> Deadline {
    PROCESS_DEADLINE
        .get()
        .copied()
        .unwrap_or_else(Deadline::unbounded)
}

/// A budget that was exhausted while the named scope was in flight.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{scope} exceeded the {budget_ms}ms budget after {elapsed_ms}ms ({completed} completed)")]
pub struct TimeoutError {
    /// What was being processed when the budget ran out.
    pub scope: String,
    pub budget_ms: u64,
    pub elapsed_ms: u64,
    /// How many units finished before the budget ran out, so a caller can
    /// report progress instead of only failure.
    pub completed: usize,
}

/// A wall-clock budget for one invocation.
///
/// Cloning shares the same start instant, so passing a deadline down does not
/// hand the callee a fresh budget.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    started: Option<Instant>,
    budget: Option<Duration>,
}

impl Default for Deadline {
    fn default() -> Self {
        Self::unbounded()
    }
}

impl Deadline {
    /// A deadline that never expires and never reads the clock.
    #[must_use]
    pub const fn unbounded() -> Self {
        Self {
            started: None,
            budget: None,
        }
    }

    /// Starts a budget now, or returns an unbounded deadline when `None`.
    ///
    /// A budget of zero is honoured rather than rejected: `--timeout-ms 0`
    /// means "do nothing that could take time", which is a legitimate way to
    /// ask whether a run would start at all.
    #[must_use]
    pub fn from_millis(budget_ms: Option<u64>) -> Self {
        match budget_ms {
            Some(budget_ms) => Self {
                started: Some(Instant::now()),
                budget: Some(Duration::from_millis(budget_ms)),
            },
            None => Self::unbounded(),
        }
    }

    /// Whether a budget was armed at all.
    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.budget.is_some()
    }

    /// The configured budget in milliseconds, if armed.
    #[must_use]
    pub fn budget_ms(&self) -> Option<u64> {
        self.budget.map(millis)
    }

    /// Milliseconds since the budget started, or zero when unarmed.
    #[must_use]
    pub fn elapsed_ms(&self) -> u64 {
        self.started.map_or(0, |started| millis(started.elapsed()))
    }

    /// What remains of the budget, or `None` when unarmed.
    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        let (started, budget) = (self.started?, self.budget?);
        Some(budget.saturating_sub(started.elapsed()))
    }

    /// Whether the budget has run out. Always false when unarmed.
    #[must_use]
    pub fn expired(&self) -> bool {
        match (self.started, self.budget) {
            (Some(started), Some(budget)) => started.elapsed() >= budget,
            _ => false,
        }
    }

    /// Fails when the budget has run out, naming what was in flight.
    ///
    /// `completed` is the count of units already finished; it appears in the
    /// message so "timed out" can be read as "timed out having done 412 of
    /// 9,000 files" without a second progress channel.
    pub fn check(&self, scope: impl fmt::Display, completed: usize) -> Result<(), TimeoutError> {
        let (Some(started), Some(budget)) = (self.started, self.budget) else {
            return Ok(());
        };
        let elapsed = started.elapsed();
        if elapsed < budget {
            return Ok(());
        }
        Err(TimeoutError {
            scope: scope.to_string(),
            budget_ms: millis(budget),
            elapsed_ms: millis(elapsed),
            completed,
        })
    }
}

/// A duration in whole milliseconds, saturating rather than wrapping.
///
/// `Duration::as_millis` is `u128`; every consumer here reports a `u64`. A
/// duration that does not fit is not a real budget, so saturating is the
/// honest reduction.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Deadline, TimeoutError};
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn an_unbounded_deadline_never_expires() {
        let deadline = Deadline::unbounded();
        assert!(!deadline.is_armed());
        assert!(!deadline.expired());
        assert_eq!(deadline.budget_ms(), None);
        assert_eq!(deadline.remaining(), None);
        assert_eq!(deadline.elapsed_ms(), 0);
        assert_eq!(deadline.check("scan", 0), Ok(()));
    }

    #[test]
    fn the_default_deadline_is_the_unbounded_one() {
        assert!(!Deadline::default().is_armed());
    }

    #[test]
    fn no_budget_means_unbounded() {
        assert!(!Deadline::from_millis(None).is_armed());
    }

    #[test]
    fn a_generous_budget_does_not_fire() {
        let deadline = Deadline::from_millis(Some(60_000));
        assert!(deadline.is_armed());
        assert_eq!(deadline.budget_ms(), Some(60_000));
        assert_eq!(deadline.check("scan src/core.lisp", 3), Ok(()));
        assert!(
            deadline
                .remaining()
                .is_some_and(|left| left.as_millis() > 0)
        );
    }

    /// Zero is a real budget, not a synonym for "off". Rejecting it would make
    /// `--timeout-ms 0` mean the opposite of what it reads as.
    #[test]
    fn a_zero_budget_expires_immediately_and_names_the_scope() {
        let deadline = Deadline::from_millis(Some(0));
        assert!(deadline.is_armed());
        assert!(deadline.expired());

        let error = deadline
            .check("scan src/core.lisp", 7)
            .expect_err("a zero budget must expire");
        assert_eq!(
            error,
            TimeoutError {
                scope: "scan src/core.lisp".to_owned(),
                budget_ms: 0,
                elapsed_ms: error.elapsed_ms,
                completed: 7,
            }
        );
        assert!(
            error.to_string().contains("scan src/core.lisp"),
            "unexpected message: {error}"
        );
        assert!(
            error.to_string().contains("7 completed"),
            "the message must carry progress: {error}"
        );
    }

    #[test]
    fn a_budget_that_elapses_starts_failing() {
        let deadline = Deadline::from_millis(Some(1));
        sleep(Duration::from_millis(5));
        assert!(deadline.expired());
        assert_eq!(deadline.remaining(), Some(Duration::ZERO));
        assert!(deadline.check("lint", 0).is_err());
    }

    /// A copied deadline must not restart the clock; otherwise handing it to a
    /// per-file helper would grant each file the whole budget.
    #[test]
    fn copying_a_deadline_shares_the_original_start() {
        let deadline = Deadline::from_millis(Some(1));
        let handed_down = deadline;
        sleep(Duration::from_millis(5));
        assert!(handed_down.expired());
    }
}
