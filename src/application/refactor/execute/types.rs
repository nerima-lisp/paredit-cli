#[derive(Debug, Clone, Copy)]
pub struct RefactorWriteCandidate {
    pub changed: bool,
    pub output_parse_ok: bool,
}

pub use crate::domain::refactor_execute::{
    RefactorExecuteDecision, RefactorExecuteDecisionStatus, RefactorExecuteGateInputs,
    RefactorExecuteMode, RefactorExecuteOutcome, RefactorExecuteOutputParseResult,
    RefactorExecutePolicyResult, RefactorExecutePostVerificationResult,
    RefactorExecutePreVerificationResult, RefactorExecutePreflightInputs, RefactorExecuteStep,
    RefactorExecuteStepStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefactorWriteRefusal {
    UnparsableOutputs { count: usize },
}

impl RefactorWriteRefusal {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::UnparsableOutputs { .. } => "unparsable-outputs",
        }
    }

    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self {
            Self::UnparsableOutputs { .. } => "rewritten-output-did-not-parse",
        }
    }

    #[must_use]
    pub fn next_action(&self) -> &'static str {
        match self {
            Self::UnparsableOutputs { .. } => "inspect-preview-parse-errors",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RefactorWritePlanState {
    NotRequested,
    Refused(RefactorWriteRefusal),
    Allowed { writable_indexes: Vec<usize> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorWritePlan {
    state: RefactorWritePlanState,
}

impl RefactorWritePlan {
    pub(super) fn not_requested() -> Self {
        Self {
            state: RefactorWritePlanState::NotRequested,
        }
    }

    pub(super) fn refused(refusal: RefactorWriteRefusal) -> Self {
        Self {
            state: RefactorWritePlanState::Refused(refusal),
        }
    }

    pub(super) fn allowed(writable_indexes: Vec<usize>) -> Self {
        Self {
            state: RefactorWritePlanState::Allowed { writable_indexes },
        }
    }

    #[must_use]
    pub fn write_requested(&self) -> bool {
        !matches!(self.state, RefactorWritePlanState::NotRequested)
    }

    #[must_use]
    pub fn write_allowed(&self) -> bool {
        matches!(self.state, RefactorWritePlanState::Allowed { .. })
    }

    #[must_use]
    pub fn writable_indexes(&self) -> &[usize] {
        match &self.state {
            RefactorWritePlanState::Allowed { writable_indexes } => writable_indexes,
            RefactorWritePlanState::NotRequested | RefactorWritePlanState::Refused(_) => &[],
        }
    }

    #[must_use]
    pub fn refusal(&self) -> Option<&RefactorWriteRefusal> {
        match &self.state {
            RefactorWritePlanState::Refused(refusal) => Some(refusal),
            RefactorWritePlanState::NotRequested | RefactorWritePlanState::Allowed { .. } => None,
        }
    }
}
