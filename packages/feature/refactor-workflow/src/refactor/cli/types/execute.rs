use super::preview::RefactorPreview;
use super::verification::RefactorVerification;
use crate::application::refactor::execute::{
    RefactorExecuteDecision, RefactorExecuteOutcome, RefactorExecutePostVerificationResult,
    RefactorExecuteStep, RefactorExecuteStepStatus,
};

#[derive(Debug)]
pub(in crate::presentation::cli) struct WorkspaceRefactorExecute {
    pub(in crate::presentation::cli) preview: RefactorPreview,
    pub(in crate::presentation::cli) preflight_decision: RefactorExecuteDecision,
    pub(in crate::presentation::cli) execute_decision: RefactorExecuteDecision,
    pub(in crate::presentation::cli) outcome: WorkspaceRefactorExecuteOutcome,
    pub(in crate::presentation::cli) pre_verification: Option<RefactorVerification>,
    pub(in crate::presentation::cli) post_verification: Option<RefactorVerification>,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct WorkspaceRefactorExecuteOutcome {
    status: WorkspaceRefactorExecuteOutcomeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::presentation::cli) struct WorkspaceRefactorExecuteOutcomeSummary {
    passed_step_count: usize,
    failed_step_count: usize,
    skipped_step_count: usize,
    scheduled_step_count: usize,
    write_applied: bool,
    post_verification_passed: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::presentation::cli) enum WorkspaceRefactorExecuteOutcomeStatus {
    BlockedByPolicy,
    RefusedUnparsableOutput,
    BlockedByPreVerification,
    DryRunReady,
    WriteApplied,
    PostVerificationFailed,
}

impl WorkspaceRefactorExecuteOutcome {
    pub(in crate::presentation::cli) fn from_decision(
        decision: RefactorExecuteDecision,
        post_verification: Option<RefactorExecutePostVerificationResult>,
    ) -> Result<Self, &'static str> {
        let status = match RefactorExecuteOutcome::from_decision(decision, post_verification)? {
            RefactorExecuteOutcome::BlockedByPolicy => {
                WorkspaceRefactorExecuteOutcomeStatus::BlockedByPolicy
            }
            RefactorExecuteOutcome::RefusedUnparsableOutput => {
                WorkspaceRefactorExecuteOutcomeStatus::RefusedUnparsableOutput
            }
            RefactorExecuteOutcome::BlockedByPreVerification => {
                WorkspaceRefactorExecuteOutcomeStatus::BlockedByPreVerification
            }
            RefactorExecuteOutcome::DryRunReady => {
                WorkspaceRefactorExecuteOutcomeStatus::DryRunReady
            }
            RefactorExecuteOutcome::WriteApplied => {
                WorkspaceRefactorExecuteOutcomeStatus::WriteApplied
            }
            RefactorExecuteOutcome::PostVerificationFailed => {
                WorkspaceRefactorExecuteOutcomeStatus::PostVerificationFailed
            }
        };
        Ok(Self { status })
    }

    pub(in crate::presentation::cli) const fn status(
        &self,
    ) -> WorkspaceRefactorExecuteOutcomeStatus {
        self.status
    }

    pub(in crate::presentation::cli) const fn write_applied(&self) -> bool {
        matches!(
            self.status,
            WorkspaceRefactorExecuteOutcomeStatus::WriteApplied
                | WorkspaceRefactorExecuteOutcomeStatus::PostVerificationFailed
        )
    }

    pub(in crate::presentation::cli) const fn post_verification_passed(&self) -> Option<bool> {
        match self.status {
            WorkspaceRefactorExecuteOutcomeStatus::WriteApplied => Some(true),
            WorkspaceRefactorExecuteOutcomeStatus::PostVerificationFailed => Some(false),
            _ => None,
        }
    }

    pub(in crate::presentation::cli) fn steps(&self) -> Vec<RefactorExecuteStep> {
        use RefactorExecuteStepStatus::{Failed, Passed, Scheduled, Skipped};
        use WorkspaceRefactorExecuteOutcomeStatus::{
            BlockedByPolicy, BlockedByPreVerification, DryRunReady, PostVerificationFailed,
            RefusedUnparsableOutput, WriteApplied,
        };

        let status = self.status;
        let preview_policy = if matches!(status, BlockedByPolicy) {
            Failed
        } else {
            Passed
        };
        let output_parse = match status {
            BlockedByPolicy => Skipped,
            RefusedUnparsableOutput => Failed,
            BlockedByPreVerification | DryRunReady | WriteApplied | PostVerificationFailed => {
                Passed
            }
        };
        let pre_verification = match status {
            BlockedByPolicy | RefusedUnparsableOutput => Skipped,
            BlockedByPreVerification => Failed,
            DryRunReady | WriteApplied | PostVerificationFailed => Passed,
        };
        let apply_preview = match status {
            BlockedByPolicy | RefusedUnparsableOutput | BlockedByPreVerification => Skipped,
            DryRunReady => Scheduled,
            WriteApplied | PostVerificationFailed => Passed,
        };
        let post_verification = match status {
            WriteApplied => Passed,
            PostVerificationFailed => Failed,
            BlockedByPolicy | RefusedUnparsableOutput | BlockedByPreVerification | DryRunReady => {
                Skipped
            }
        };

        vec![
            RefactorExecuteStep {
                name: "preview-policy",
                status: preview_policy,
            },
            RefactorExecuteStep {
                name: "write-output-parse",
                status: output_parse,
            },
            RefactorExecuteStep {
                name: "pre-verification",
                status: pre_verification,
            },
            RefactorExecuteStep {
                name: "apply-preview",
                status: apply_preview,
            },
            RefactorExecuteStep {
                name: "post-verification",
                status: post_verification,
            },
        ]
    }

    pub(in crate::presentation::cli) fn summary(&self) -> WorkspaceRefactorExecuteOutcomeSummary {
        let mut summary = WorkspaceRefactorExecuteOutcomeSummary {
            passed_step_count: 0,
            failed_step_count: 0,
            skipped_step_count: 0,
            scheduled_step_count: 0,
            write_applied: self.write_applied(),
            post_verification_passed: self.post_verification_passed(),
        };

        for step in self.steps() {
            match step.status {
                RefactorExecuteStepStatus::Passed => summary.passed_step_count += 1,
                RefactorExecuteStepStatus::Failed => summary.failed_step_count += 1,
                RefactorExecuteStepStatus::Skipped => summary.skipped_step_count += 1,
                RefactorExecuteStepStatus::Scheduled => summary.scheduled_step_count += 1,
            }
        }

        summary
    }
}

impl WorkspaceRefactorExecuteOutcomeSummary {
    pub(in crate::presentation::cli) const fn passed_step_count(self) -> usize {
        self.passed_step_count
    }
    pub(in crate::presentation::cli) const fn failed_step_count(self) -> usize {
        self.failed_step_count
    }
    pub(in crate::presentation::cli) const fn skipped_step_count(self) -> usize {
        self.skipped_step_count
    }
    pub(in crate::presentation::cli) const fn scheduled_step_count(self) -> usize {
        self.scheduled_step_count
    }
    pub(in crate::presentation::cli) const fn write_applied(self) -> bool {
        self.write_applied
    }
    pub(in crate::presentation::cli) const fn post_verification_passed(self) -> Option<bool> {
        self.post_verification_passed
    }
}

impl WorkspaceRefactorExecuteOutcomeStatus {
    pub(in crate::presentation::cli) const fn label(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "blocked-by-policy",
            Self::RefusedUnparsableOutput => "refused-unparsable-output",
            Self::BlockedByPreVerification => "blocked-by-pre-verification",
            Self::DryRunReady => "dry-run-ready",
            Self::WriteApplied => "write-applied",
            Self::PostVerificationFailed => "post-verification-failed",
        }
    }

    pub(in crate::presentation::cli) const fn reason(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "preview-policy-failed",
            Self::RefusedUnparsableOutput => "rewritten-output-did-not-parse",
            Self::BlockedByPreVerification => "pre-verification-failed",
            Self::DryRunReady => "all-dry-run-gates-passed",
            Self::WriteApplied => "write-and-post-verification-passed",
            Self::PostVerificationFailed => "post-verification-failed",
        }
    }

    pub(in crate::presentation::cli) const fn next_action(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "review-policy-violations",
            Self::RefusedUnparsableOutput => "inspect-preview-parse-errors",
            Self::BlockedByPreVerification => "review-pre-verification-checks",
            Self::DryRunReady => "review-preview-or-rerun-with-write",
            Self::WriteApplied => "review-written-files",
            Self::PostVerificationFailed => "review-post-verification-checks",
        }
    }
}
