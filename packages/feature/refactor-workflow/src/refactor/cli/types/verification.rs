use crate::refactor::usecase::plan::RefactorPlanTargetKind;
use crate::refactor::usecase::plan::{
    RefactorOperation as ApplicationRefactorOperation, RefactorPlanSummary,
    RefactorVerificationCheck, VerificationPhase as ApplicationVerificationPhase,
};

#[derive(Debug)]
pub struct RefactorVerification {
    pub operation: ApplicationRefactorOperation,
    pub phase: ApplicationVerificationPhase,
    pub symbol: String,
    pub new_symbol: Option<String>,
    pub passed: bool,
    pub target_kind: RefactorPlanTargetKind,
    pub checks: Vec<RefactorVerificationCheck>,
    pub before: RefactorPlanSummary,
    pub after: Option<RefactorPlanSummary>,
}
