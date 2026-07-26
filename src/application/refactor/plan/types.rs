use std::path::PathBuf;

use crate::domain::refactor_plan::{
    RawRefactorRisk, RefactorOperation, RefactorPlanGate, RefactorPlanPolicy,
    RefactorPlanPolicyOptions, RefactorPlanRiskSummary, RefactorPlanSummary,
    RefactorPlanTargetKind,
};

#[derive(Debug, Clone)]
pub struct RefactorPlanStep {
    pub order: usize,
    pub action: &'static str,
    pub rationale: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorPlanAutomationStatus {
    Ready,
    ManualReview,
    PolicyFailed,
}

impl RefactorPlanAutomationStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::ManualReview => "manual_review",
            Self::PolicyFailed => "policy_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorPlanAutomationStepStatus {
    Passed,
    Failed,
    Skipped,
    Scheduled,
}

impl RefactorPlanAutomationStepStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorPlanAutomationStep {
    pub name: &'static str,
    pub status: RefactorPlanAutomationStepStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RefactorPlanAutomationState {
    PolicyFailed {
        reason: String,
        blocking_gate_count: usize,
    },
    ManualReview {
        reason: String,
        next_action: &'static str,
        blocking_gate_count: usize,
    },
    Ready {
        next_action: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorPlanAutomationDecision {
    state: RefactorPlanAutomationState,
}

impl RefactorPlanAutomationDecision {
    pub(super) fn policy_failed(reason: String, blocking_gate_count: usize) -> Self {
        Self {
            state: RefactorPlanAutomationState::PolicyFailed {
                reason,
                blocking_gate_count,
            },
        }
    }

    pub(super) fn manual_review(
        reason: String,
        next_action: &'static str,
        blocking_gate_count: usize,
    ) -> Self {
        Self {
            state: RefactorPlanAutomationState::ManualReview {
                reason,
                next_action,
                blocking_gate_count,
            },
        }
    }

    pub(super) fn ready(next_action: &'static str) -> Self {
        Self {
            state: RefactorPlanAutomationState::Ready { next_action },
        }
    }

    #[must_use]
    pub fn status(&self) -> RefactorPlanAutomationStatus {
        match self.state {
            RefactorPlanAutomationState::PolicyFailed { .. } => {
                RefactorPlanAutomationStatus::PolicyFailed
            }
            RefactorPlanAutomationState::ManualReview { .. } => {
                RefactorPlanAutomationStatus::ManualReview
            }
            RefactorPlanAutomationState::Ready { .. } => RefactorPlanAutomationStatus::Ready,
        }
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        match &self.state {
            RefactorPlanAutomationState::PolicyFailed { reason, .. }
            | RefactorPlanAutomationState::ManualReview { reason, .. } => reason,
            RefactorPlanAutomationState::Ready { .. } => {
                "policy passed and no blocking gates were found"
            }
        }
    }

    #[must_use]
    pub fn next_action(&self) -> &'static str {
        match self.state {
            RefactorPlanAutomationState::PolicyFailed { .. } => "resolve-policy-violations",
            RefactorPlanAutomationState::ManualReview { next_action, .. }
            | RefactorPlanAutomationState::Ready { next_action } => next_action,
        }
    }

    #[must_use]
    pub fn safe_to_automate(&self) -> bool {
        matches!(self.state, RefactorPlanAutomationState::Ready { .. })
    }

    #[must_use]
    pub fn policy_passed(&self) -> bool {
        !matches!(self.state, RefactorPlanAutomationState::PolicyFailed { .. })
    }

    #[must_use]
    pub fn blocking_gate_count(&self) -> usize {
        match self.state {
            RefactorPlanAutomationState::PolicyFailed {
                blocking_gate_count,
                ..
            }
            | RefactorPlanAutomationState::ManualReview {
                blocking_gate_count,
                ..
            } => blocking_gate_count,
            RefactorPlanAutomationState::Ready { .. } => 0,
        }
    }

    #[must_use]
    pub fn steps(&self) -> [RefactorPlanAutomationStep; 3] {
        let policy_status = if self.policy_passed() {
            RefactorPlanAutomationStepStatus::Passed
        } else {
            RefactorPlanAutomationStepStatus::Failed
        };
        let review_status = match self.status() {
            RefactorPlanAutomationStatus::Ready => RefactorPlanAutomationStepStatus::Passed,
            RefactorPlanAutomationStatus::ManualReview => {
                RefactorPlanAutomationStepStatus::Scheduled
            }
            RefactorPlanAutomationStatus::PolicyFailed => RefactorPlanAutomationStepStatus::Skipped,
        };
        let apply_status = if self.safe_to_automate() {
            RefactorPlanAutomationStepStatus::Scheduled
        } else {
            RefactorPlanAutomationStepStatus::Skipped
        };

        [
            RefactorPlanAutomationStep {
                name: "plan-policy",
                status: policy_status,
            },
            RefactorPlanAutomationStep {
                name: "manual-review-gates",
                status: review_status,
            },
            RefactorPlanAutomationStep {
                name: "apply-plan",
                status: apply_status,
            },
        ]
    }
}

#[derive(Debug, Clone)]
pub struct RefactorPlanRequest<'a> {
    pub operation: RefactorOperation,
    pub symbol: &'a str,
    pub files: &'a [PathBuf],
    pub target_kind: RefactorPlanTargetKind,
    pub summary: RefactorPlanSummary,
    pub policy: RefactorPlanPolicyOptions,
    pub risks: Vec<RawRefactorRisk>,
}

#[derive(Debug, Clone)]
pub struct RefactorPlanDecision {
    pub gates: Vec<RefactorPlanGate>,
    pub risk_summary: RefactorPlanRiskSummary,
    pub steps: Vec<RefactorPlanStep>,
    pub policy: RefactorPlanPolicy,
    pub automation: RefactorPlanAutomationDecision,
}
