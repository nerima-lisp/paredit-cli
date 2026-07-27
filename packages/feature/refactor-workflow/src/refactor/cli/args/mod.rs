pub(in crate::presentation::cli) mod execute;
pub(in crate::presentation::cli) mod manifest;
pub(in crate::presentation::cli) mod plan;
pub(in crate::presentation::cli) mod preview;
pub(in crate::presentation::cli) mod verification;

pub(in crate::presentation::cli) use execute::WorkspaceRefactorExecuteArgs;
pub(in crate::presentation::cli) use manifest::{
    RefactorApplyArgs, RefactorCheckArgs, RefactorDiffArgs, RefactorStatusArgs,
};
pub(in crate::presentation::cli) use plan::{
    RefactorOperation, RefactorPlanArgs, WorkspaceRefactorPlanArgs,
};
pub(in crate::presentation::cli) use preview::{
    RefactorPreviewArgs, RefactorPreviewMode, WorkspaceRefactorPreviewArgs,
};
pub(in crate::presentation::cli) use verification::{VerificationPhase, VerifyRefactorArgs};
