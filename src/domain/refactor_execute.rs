#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecuteMode {
    DryRun,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecutePolicyResult {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecuteOutputParseResult {
    Parseable,
    Unparsable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecutePreVerificationResult {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecutePostVerificationResult {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorExecutePreflightInputs {
    mode: RefactorExecuteMode,
    policy: RefactorExecutePolicyResult,
    output_parse: RefactorExecuteOutputParseResult,
}

impl RefactorExecutePreflightInputs {
    #[must_use]
    pub const fn new(
        mode: RefactorExecuteMode,
        policy: RefactorExecutePolicyResult,
        output_parse: RefactorExecuteOutputParseResult,
    ) -> Self {
        Self {
            mode,
            policy,
            output_parse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorExecuteGateInputs {
    mode: RefactorExecuteMode,
    policy: RefactorExecutePolicyResult,
    output_parse: RefactorExecuteOutputParseResult,
    pre_verification: RefactorExecutePreVerificationResult,
}

impl RefactorExecuteGateInputs {
    #[must_use]
    pub const fn new(
        mode: RefactorExecuteMode,
        policy: RefactorExecutePolicyResult,
        output_parse: RefactorExecuteOutputParseResult,
        pre_verification: RefactorExecutePreVerificationResult,
    ) -> Self {
        Self {
            mode,
            policy,
            output_parse,
            pre_verification,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecuteDecisionStatus {
    BlockedByPolicy,
    RefusedUnparsableOutput,
    BlockedByPreVerification,
    ReadyToWrite,
    DryRunReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecuteOutcome {
    BlockedByPolicy,
    RefusedUnparsableOutput,
    BlockedByPreVerification,
    DryRunReady,
    WriteApplied,
    PostVerificationFailed,
}

impl RefactorExecuteOutcome {
    pub const fn from_decision(
        decision: RefactorExecuteDecision,
        post_verification: Option<RefactorExecutePostVerificationResult>,
    ) -> Result<Self, &'static str> {
        match (decision.status(), post_verification) {
            (RefactorExecuteDecisionStatus::BlockedByPolicy, None) => Ok(Self::BlockedByPolicy),
            (RefactorExecuteDecisionStatus::RefusedUnparsableOutput, None) => {
                Ok(Self::RefusedUnparsableOutput)
            }
            (RefactorExecuteDecisionStatus::BlockedByPreVerification, None) => {
                Ok(Self::BlockedByPreVerification)
            }
            (RefactorExecuteDecisionStatus::DryRunReady, None) => Ok(Self::DryRunReady),
            (
                RefactorExecuteDecisionStatus::ReadyToWrite,
                Some(RefactorExecutePostVerificationResult::Passed),
            ) => Ok(Self::WriteApplied),
            (
                RefactorExecuteDecisionStatus::ReadyToWrite,
                Some(RefactorExecutePostVerificationResult::Failed),
            ) => Ok(Self::PostVerificationFailed),
            (RefactorExecuteDecisionStatus::ReadyToWrite, None) => {
                Err("write decision requires post-verification result")
            }
            (_, Some(_)) => Err("non-write decision cannot have post-verification result"),
        }
    }

    #[must_use]
    pub const fn write_applied(self) -> bool {
        matches!(self, Self::WriteApplied | Self::PostVerificationFailed)
    }

    #[must_use]
    pub const fn post_verification_passed(self) -> Option<bool> {
        match self {
            Self::WriteApplied => Some(true),
            Self::PostVerificationFailed => Some(false),
            _ => None,
        }
    }

    #[must_use]
    pub const fn steps(self) -> [RefactorExecuteStep; 5] {
        use RefactorExecuteStepStatus::{Failed, Passed, Scheduled, Skipped};
        let preview_policy = if matches!(self, Self::BlockedByPolicy) {
            Failed
        } else {
            Passed
        };
        let output_parse = match self {
            Self::BlockedByPolicy => Skipped,
            Self::RefusedUnparsableOutput => Failed,
            _ => Passed,
        };
        let pre_verification = match self {
            Self::BlockedByPolicy | Self::RefusedUnparsableOutput => Skipped,
            Self::BlockedByPreVerification => Failed,
            _ => Passed,
        };
        let apply_preview = match self {
            Self::BlockedByPolicy
            | Self::RefusedUnparsableOutput
            | Self::BlockedByPreVerification => Skipped,
            Self::DryRunReady => Scheduled,
            Self::WriteApplied | Self::PostVerificationFailed => Passed,
        };
        let post_verification = match self {
            Self::WriteApplied => Passed,
            Self::PostVerificationFailed => Failed,
            _ => Skipped,
        };
        [
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
}

impl RefactorExecuteDecisionStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "blocked-by-policy",
            Self::RefusedUnparsableOutput => "refused-unparsable-output",
            Self::BlockedByPreVerification => "blocked-by-pre-verification",
            Self::ReadyToWrite => "ready-to-write",
            Self::DryRunReady => "dry-run-ready",
        }
    }

    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "preview-policy-failed",
            Self::RefusedUnparsableOutput => "rewritten-output-did-not-parse",
            Self::BlockedByPreVerification => "pre-verification-failed",
            Self::ReadyToWrite => "all-execute-gates-passed",
            Self::DryRunReady => "all-dry-run-gates-passed",
        }
    }

    #[must_use]
    pub const fn next_action(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "review-policy-violations",
            Self::RefusedUnparsableOutput => "inspect-preview-parse-errors",
            Self::BlockedByPreVerification => "review-pre-verification-checks",
            Self::ReadyToWrite => "write-preview-and-run-post-verification",
            Self::DryRunReady => "review-preview-or-rerun-with-write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorExecuteStepStatus {
    Passed,
    Failed,
    Skipped,
    Scheduled,
}

impl RefactorExecuteStepStatus {
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
pub struct RefactorExecuteStep {
    pub name: &'static str,
    pub status: RefactorExecuteStepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorExecuteDecisionSummary {
    passed_step_count: usize,
    failed_step_count: usize,
    skipped_step_count: usize,
    scheduled_step_count: usize,
    write_parse_refused: bool,
    run_pre_verification: bool,
    apply_preview: bool,
    run_post_verification: bool,
}

impl RefactorExecuteDecisionSummary {
    #[must_use]
    pub const fn passed_step_count(self) -> usize {
        self.passed_step_count
    }
    #[must_use]
    pub const fn failed_step_count(self) -> usize {
        self.failed_step_count
    }
    #[must_use]
    pub const fn skipped_step_count(self) -> usize {
        self.skipped_step_count
    }
    #[must_use]
    pub const fn scheduled_step_count(self) -> usize {
        self.scheduled_step_count
    }
    #[must_use]
    pub const fn write_parse_refused(self) -> bool {
        self.write_parse_refused
    }
    #[must_use]
    pub const fn run_pre_verification(self) -> bool {
        self.run_pre_verification
    }
    #[must_use]
    pub const fn apply_preview(self) -> bool {
        self.apply_preview
    }
    #[must_use]
    pub const fn run_post_verification(self) -> bool {
        self.run_post_verification
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorExecuteDecision {
    status: RefactorExecuteDecisionStatus,
}

impl RefactorExecuteDecision {
    #[must_use]
    pub const fn status(self) -> RefactorExecuteDecisionStatus {
        self.status
    }

    #[must_use]
    pub const fn write_parse_refused(self) -> bool {
        matches!(
            self.status,
            RefactorExecuteDecisionStatus::RefusedUnparsableOutput
        )
    }

    #[must_use]
    pub const fn run_pre_verification(self) -> bool {
        matches!(
            self.status,
            RefactorExecuteDecisionStatus::BlockedByPreVerification
                | RefactorExecuteDecisionStatus::ReadyToWrite
                | RefactorExecuteDecisionStatus::DryRunReady
        )
    }

    #[must_use]
    pub const fn apply_preview(self) -> bool {
        matches!(
            self.status,
            RefactorExecuteDecisionStatus::ReadyToWrite
                | RefactorExecuteDecisionStatus::DryRunReady
        )
    }

    #[must_use]
    pub const fn run_post_verification(self) -> bool {
        matches!(self.status, RefactorExecuteDecisionStatus::ReadyToWrite)
    }

    #[must_use]
    pub fn summary(self) -> RefactorExecuteDecisionSummary {
        let mut summary = RefactorExecuteDecisionSummary {
            passed_step_count: 0,
            failed_step_count: 0,
            skipped_step_count: 0,
            scheduled_step_count: 0,
            write_parse_refused: self.write_parse_refused(),
            run_pre_verification: self.run_pre_verification(),
            apply_preview: self.apply_preview(),
            run_post_verification: self.run_post_verification(),
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

    #[must_use]
    pub fn steps(self) -> [RefactorExecuteStep; 5] {
        [
            RefactorExecuteStep {
                name: "preview-policy",
                status: if self.status == RefactorExecuteDecisionStatus::BlockedByPolicy {
                    RefactorExecuteStepStatus::Failed
                } else {
                    RefactorExecuteStepStatus::Passed
                },
            },
            RefactorExecuteStep {
                name: "write-output-parse",
                status: if self.status == RefactorExecuteDecisionStatus::BlockedByPolicy {
                    RefactorExecuteStepStatus::Skipped
                } else if self.write_parse_refused() {
                    RefactorExecuteStepStatus::Failed
                } else {
                    RefactorExecuteStepStatus::Passed
                },
            },
            RefactorExecuteStep {
                name: "pre-verification",
                status: if self.status == RefactorExecuteDecisionStatus::BlockedByPreVerification {
                    RefactorExecuteStepStatus::Failed
                } else if self.run_pre_verification() {
                    RefactorExecuteStepStatus::Passed
                } else {
                    RefactorExecuteStepStatus::Skipped
                },
            },
            RefactorExecuteStep {
                name: "apply-preview",
                status: if self.apply_preview() {
                    RefactorExecuteStepStatus::Scheduled
                } else {
                    RefactorExecuteStepStatus::Skipped
                },
            },
            RefactorExecuteStep {
                name: "post-verification",
                status: if self.run_post_verification() {
                    RefactorExecuteStepStatus::Scheduled
                } else {
                    RefactorExecuteStepStatus::Skipped
                },
            },
        ]
    }
}

#[must_use]
pub const fn build_refactor_execute_decision(
    inputs: RefactorExecuteGateInputs,
) -> RefactorExecuteDecision {
    let status = match (
        inputs.policy,
        inputs.mode,
        inputs.output_parse,
        inputs.pre_verification,
    ) {
        (RefactorExecutePolicyResult::Failed, _, _, _) => {
            RefactorExecuteDecisionStatus::BlockedByPolicy
        }
        (
            RefactorExecutePolicyResult::Passed,
            RefactorExecuteMode::Write,
            RefactorExecuteOutputParseResult::Unparsable,
            _,
        ) => RefactorExecuteDecisionStatus::RefusedUnparsableOutput,
        (
            RefactorExecutePolicyResult::Passed,
            _,
            _,
            RefactorExecutePreVerificationResult::Failed,
        ) => RefactorExecuteDecisionStatus::BlockedByPreVerification,
        (
            RefactorExecutePolicyResult::Passed,
            RefactorExecuteMode::Write,
            _,
            RefactorExecutePreVerificationResult::Passed,
        ) => RefactorExecuteDecisionStatus::ReadyToWrite,
        (
            RefactorExecutePolicyResult::Passed,
            RefactorExecuteMode::DryRun,
            _,
            RefactorExecutePreVerificationResult::Passed,
        ) => RefactorExecuteDecisionStatus::DryRunReady,
    };

    RefactorExecuteDecision { status }
}

#[must_use]
pub const fn build_refactor_execute_preflight_decision(
    inputs: RefactorExecutePreflightInputs,
) -> RefactorExecuteDecision {
    build_refactor_execute_decision(RefactorExecuteGateInputs::new(
        inputs.mode,
        inputs.policy,
        inputs.output_parse,
        RefactorExecutePreVerificationResult::Passed,
    ))
}
