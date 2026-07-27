use super::super::super::*;
use super::check::{RefactorCheckFileResult, RefactorCheckSummary};
use super::manifest::RefactorApplyManifestHeader;
use super::root::RefactorRootReport;

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorStatusResult {
    pub(in crate::presentation::cli) manifest: RefactorApplyManifestHeader,
    pub(in crate::presentation::cli) root: RefactorRootReport,
    pub(in crate::presentation::cli) manifest_policy_passed: bool,
    pub(in crate::presentation::cli) manifest_outputs_parse: bool,
    pub(in crate::presentation::cli) status: RefactorStatusKind,
    pub(in crate::presentation::cli) next_action: RefactorStatusNextAction,
    pub(in crate::presentation::cli) blocked_reasons: Vec<RefactorStatusBlockedReason>,
    pub(in crate::presentation::cli) write_plan: Vec<RefactorStatusWriteTarget>,
    pub(in crate::presentation::cli) files: Vec<RefactorCheckFileResult>,
    pub(in crate::presentation::cli) summary: RefactorCheckSummary,
}

impl RefactorStatusResult {
    pub(in crate::presentation::cli) fn steps(&self) -> [RefactorManifestDecisionStep; 5] {
        RefactorManifestDecision::steps_from_blocked_reasons(&self.blocked_reasons)
    }

    pub(in crate::presentation::cli) fn decision_summary(&self) -> RefactorManifestDecisionSummary {
        RefactorManifestDecisionSummary::from_steps_and_blocked_reasons(
            self.steps(),
            self.blocked_reasons.len(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) enum RefactorStatusKind {
    Ready,
    Blocked,
}

impl RefactorStatusKind {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::presentation::cli) struct RefactorManifestDecision {
    pub(in crate::presentation::cli) status: RefactorStatusKind,
    pub(in crate::presentation::cli) next_action: RefactorStatusNextAction,
    pub(in crate::presentation::cli) blocked_reasons: Vec<RefactorStatusBlockedReason>,
}

impl RefactorManifestDecision {
    pub(in crate::presentation::cli) fn steps(&self) -> [RefactorManifestDecisionStep; 5] {
        Self::steps_from_blocked_reasons(&self.blocked_reasons)
    }

    pub(in crate::presentation::cli) fn summary(&self) -> RefactorManifestDecisionSummary {
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
pub(in crate::presentation::cli) enum RefactorStatusNextAction {
    RunDiffThenApplyWrite,
    RegeneratePreview,
    FixManifestOrParser,
}

impl RefactorStatusNextAction {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::RunDiffThenApplyWrite => "run_refactor_diff_then_refactor_apply_write",
            Self::RegeneratePreview => "regenerate_refactor_preview",
            Self::FixManifestOrParser => "fix_manifest_or_parser",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::presentation::cli) struct RefactorApplyDecision {
    pub(in crate::presentation::cli) status: RefactorApplyDecisionStatus,
    pub(in crate::presentation::cli) next_action: RefactorApplyNextAction,
    pub(in crate::presentation::cli) blocked_reasons: Vec<RefactorStatusBlockedReason>,
}

impl RefactorApplyDecision {
    pub(in crate::presentation::cli) fn steps(&self) -> [RefactorManifestDecisionStep; 5] {
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

    pub(in crate::presentation::cli) fn summary(&self) -> RefactorManifestDecisionSummary {
        RefactorManifestDecisionSummary::from_steps_and_blocked_reasons(
            self.steps(),
            self.blocked_reasons.len(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) enum RefactorApplyDecisionStatus {
    Applied,
    DryRunReady,
    Blocked,
}

impl RefactorApplyDecisionStatus {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::DryRunReady => "dry-run-ready",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) enum RefactorApplyNextAction {
    RunVerificationOrReviewDiff,
    RerunWithWrite,
    RegeneratePreview,
    FixManifestOrParser,
}

impl RefactorApplyNextAction {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::RunVerificationOrReviewDiff => "run_verification_or_review_diff",
            Self::RerunWithWrite => "rerun_refactor_apply_with_write",
            Self::RegeneratePreview => "regenerate_refactor_preview",
            Self::FixManifestOrParser => "fix_manifest_or_parser",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) enum RefactorStatusBlockedReason {
    ManifestPolicyFailed,
    ManifestOutputsDoNotParse,
    StaleFiles,
    OutputHashMismatches,
    ParseErrors,
    ManifestFlagMismatches,
}

impl RefactorStatusBlockedReason {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
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
pub(in crate::presentation::cli) enum RefactorManifestDecisionStepStatus {
    Passed,
    Failed,
    Skipped,
    Scheduled,
}

impl RefactorManifestDecisionStepStatus {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) struct RefactorManifestDecisionStep {
    pub(in crate::presentation::cli) name: &'static str,
    pub(in crate::presentation::cli) status: RefactorManifestDecisionStepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) struct RefactorManifestDecisionSummary {
    pub(in crate::presentation::cli) passed_step_count: usize,
    pub(in crate::presentation::cli) failed_step_count: usize,
    pub(in crate::presentation::cli) skipped_step_count: usize,
    pub(in crate::presentation::cli) scheduled_step_count: usize,
    pub(in crate::presentation::cli) blocked_reason_count: usize,
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
pub(in crate::presentation::cli) struct RefactorStatusWriteTarget {
    pub(in crate::presentation::cli) path: PathBuf,
    pub(in crate::presentation::cli) edit_count: usize,
    pub(in crate::presentation::cli) input_hash: String,
    pub(in crate::presentation::cli) output_hash: String,
}
