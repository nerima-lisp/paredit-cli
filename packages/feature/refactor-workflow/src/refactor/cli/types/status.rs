use super::check::{RefactorCheckFileResult, RefactorCheckSummary};
use super::manifest::RefactorApplyManifestHeader;
use super::root::RefactorRootReport;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RefactorStatusResult {
    pub manifest: RefactorApplyManifestHeader,
    pub root: RefactorRootReport,
    pub manifest_policy_passed: bool,
    pub manifest_outputs_parse: bool,
    pub status: RefactorStatusKind,
    pub next_action: RefactorStatusNextAction,
    pub blocked_reasons: Vec<RefactorStatusBlockedReason>,
    pub write_plan: Vec<RefactorStatusWriteTarget>,
    pub files: Vec<RefactorCheckFileResult>,
    pub summary: RefactorCheckSummary,
}

impl RefactorStatusResult {
    #[must_use]
    pub fn steps(&self) -> [RefactorManifestDecisionStep; 5] {
        RefactorManifestDecision::steps_from_blocked_reasons(&self.blocked_reasons)
    }

    #[must_use]
    pub fn decision_summary(&self) -> RefactorManifestDecisionSummary {
        RefactorManifestDecisionSummary::from_steps_and_blocked_reasons(
            self.steps(),
            self.blocked_reasons.len(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorStatusKind {
    Ready,
    Blocked,
}

impl RefactorStatusKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorManifestDecision {
    pub status: RefactorStatusKind,
    pub next_action: RefactorStatusNextAction,
    pub blocked_reasons: Vec<RefactorStatusBlockedReason>,
}

impl RefactorManifestDecision {
    #[must_use]
    pub fn steps(&self) -> [RefactorManifestDecisionStep; 5] {
        Self::steps_from_blocked_reasons(&self.blocked_reasons)
    }

    #[must_use]
    pub fn summary(&self) -> RefactorManifestDecisionSummary {
        RefactorManifestDecisionSummary::from_steps_and_blocked_reasons(
            self.steps(),
            self.blocked_reasons.len(),
        )
    }

    fn steps_from_blocked_reasons(
        blocked_reasons: &[RefactorStatusBlockedReason],
    ) -> [RefactorManifestDecisionStep; 5] {
        let blocked = !blocked_reasons.is_empty();

        [
            RefactorManifestDecisionStep {
                name: "manifest-policy",
                status: step_status_for_reason(
                    blocked_reasons,
                    RefactorStatusBlockedReason::ManifestPolicyFailed,
                ),
            },
            RefactorManifestDecisionStep {
                name: "manifest-outputs-parse",
                status: step_status_for_reason(
                    blocked_reasons,
                    RefactorStatusBlockedReason::ManifestOutputsDoNotParse,
                ),
            },
            RefactorManifestDecisionStep {
                name: "file-freshness",
                status: step_status_for_reason(
                    blocked_reasons,
                    RefactorStatusBlockedReason::StaleFiles,
                ),
            },
            RefactorManifestDecisionStep {
                name: "output-validation",
                status: output_validation_step_status(blocked_reasons),
            },
            RefactorManifestDecisionStep {
                name: "apply-write",
                status: if blocked {
                    RefactorManifestDecisionStepStatus::Skipped
                } else {
                    RefactorManifestDecisionStepStatus::Scheduled
                },
            },
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorStatusNextAction {
    RunDiffThenApplyWrite,
    RegeneratePreview,
    FixManifestOrParser,
}

impl RefactorStatusNextAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RunDiffThenApplyWrite => "run_refactor_diff_then_refactor_apply_write",
            Self::RegeneratePreview => "regenerate_refactor_preview",
            Self::FixManifestOrParser => "fix_manifest_or_parser",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorApplyDecision {
    pub status: RefactorApplyDecisionStatus,
    pub next_action: RefactorApplyNextAction,
    pub blocked_reasons: Vec<RefactorStatusBlockedReason>,
}

impl RefactorApplyDecision {
    #[must_use]
    pub fn steps(&self) -> [RefactorManifestDecisionStep; 5] {
        let mut steps = RefactorManifestDecision::steps_from_blocked_reasons(&self.blocked_reasons);
        steps[4].status = match self.status {
            RefactorApplyDecisionStatus::Applied => RefactorManifestDecisionStepStatus::Passed,
            RefactorApplyDecisionStatus::DryRunReady => {
                RefactorManifestDecisionStepStatus::Scheduled
            }
            RefactorApplyDecisionStatus::Blocked => RefactorManifestDecisionStepStatus::Skipped,
        };
        steps
    }

    #[must_use]
    pub fn summary(&self) -> RefactorManifestDecisionSummary {
        RefactorManifestDecisionSummary::from_steps_and_blocked_reasons(
            self.steps(),
            self.blocked_reasons.len(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorApplyDecisionStatus {
    Applied,
    DryRunReady,
    Blocked,
}

impl RefactorApplyDecisionStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::DryRunReady => "dry-run-ready",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorApplyNextAction {
    RunVerificationOrReviewDiff,
    RerunWithWrite,
    RegeneratePreview,
    FixManifestOrParser,
}

impl RefactorApplyNextAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RunVerificationOrReviewDiff => "run_verification_or_review_diff",
            Self::RerunWithWrite => "rerun_refactor_apply_with_write",
            Self::RegeneratePreview => "regenerate_refactor_preview",
            Self::FixManifestOrParser => "fix_manifest_or_parser",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorStatusBlockedReason {
    ManifestPolicyFailed,
    ManifestOutputsDoNotParse,
    StaleFiles,
    OutputHashMismatches,
    ParseErrors,
    ManifestFlagMismatches,
}

impl RefactorStatusBlockedReason {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ManifestPolicyFailed => "manifest_policy_failed",
            Self::ManifestOutputsDoNotParse => "manifest_outputs_do_not_parse",
            Self::StaleFiles => "stale_files",
            Self::OutputHashMismatches => "output_hash_mismatches",
            Self::ParseErrors => "parse_errors",
            Self::ManifestFlagMismatches => "manifest_flag_mismatches",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorManifestDecisionStepStatus {
    Passed,
    Failed,
    Skipped,
    Scheduled,
}

impl RefactorManifestDecisionStepStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorManifestDecisionStep {
    pub name: &'static str,
    pub status: RefactorManifestDecisionStepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorManifestDecisionSummary {
    pub passed_step_count: usize,
    pub failed_step_count: usize,
    pub skipped_step_count: usize,
    pub scheduled_step_count: usize,
    pub blocked_reason_count: usize,
}

impl RefactorManifestDecisionSummary {
    fn from_steps_and_blocked_reasons(
        steps: [RefactorManifestDecisionStep; 5],
        blocked_reason_count: usize,
    ) -> Self {
        let mut summary = Self {
            passed_step_count: 0,
            failed_step_count: 0,
            skipped_step_count: 0,
            scheduled_step_count: 0,
            blocked_reason_count,
        };

        for step in steps {
            match step.status {
                RefactorManifestDecisionStepStatus::Passed => summary.passed_step_count += 1,
                RefactorManifestDecisionStepStatus::Failed => summary.failed_step_count += 1,
                RefactorManifestDecisionStepStatus::Skipped => summary.skipped_step_count += 1,
                RefactorManifestDecisionStepStatus::Scheduled => summary.scheduled_step_count += 1,
            }
        }

        summary
    }
}

fn step_status_for_reason(
    blocked_reasons: &[RefactorStatusBlockedReason],
    reason: RefactorStatusBlockedReason,
) -> RefactorManifestDecisionStepStatus {
    if blocked_reasons.contains(&reason) {
        RefactorManifestDecisionStepStatus::Failed
    } else {
        RefactorManifestDecisionStepStatus::Passed
    }
}

fn output_validation_step_status(
    blocked_reasons: &[RefactorStatusBlockedReason],
) -> RefactorManifestDecisionStepStatus {
    if blocked_reasons.iter().any(|reason| {
        matches!(
            reason,
            RefactorStatusBlockedReason::OutputHashMismatches
                | RefactorStatusBlockedReason::ParseErrors
                | RefactorStatusBlockedReason::ManifestFlagMismatches
        )
    }) {
        RefactorManifestDecisionStepStatus::Failed
    } else {
        RefactorManifestDecisionStepStatus::Passed
    }
}

#[derive(Debug)]
pub struct RefactorStatusWriteTarget {
    pub path: PathBuf,
    pub edit_count: usize,
    pub input_hash: String,
    pub output_hash: String,
}
