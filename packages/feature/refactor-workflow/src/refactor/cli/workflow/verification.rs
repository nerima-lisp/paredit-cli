use super::super::super::*;
use super::super::args::*;
use super::super::render::print_refactor_verification;
use super::super::types::verification::RefactorVerification;
use super::shared::derive_refactor_target_kind;
use crate::application::refactor::plan::RefactorPlanTargetKind;

pub(in crate::presentation::cli) fn verify_refactor(args: VerifyRefactorArgs) -> Result<()> {
    let verification = build_refactor_verification(
        &args.files,
        args.dialect,
        &args.symbol,
        args.new_symbol.as_ref(),
        args.operation,
        args.phase,
        None,
    )?;

    print_refactor_verification(&verification, args.output)
}

pub(super) fn build_refactor_verification(
    paths: &[PathBuf],
    dialect: Option<DialectArg>,
    symbol: &SymbolName,
    new_symbol: Option<&SymbolName>,
    operation: RefactorOperation,
    phase: VerificationPhase,
    target_kind_hint: Option<RefactorPlanTargetKind>,
) -> Result<RefactorVerification> {
    let application_operation = ApplicationRefactorOperation::from(operation);
    let application_phase = ApplicationVerificationPhase::from(phase);
    let before_files = impact_report::workflow::collect_impact_reports(paths, dialect, symbol)?;
    let before_target_kind = resolve_target_kind(
        derive_refactor_target_kind(&before_files, symbol.as_str()),
        target_kind_hint,
    );
    let mut before = summarize_impact_reports(&before_files);
    let gates = application_refactor_plan_gates(
        application_operation,
        before_target_kind,
        &before,
        raw_refactor_risks(&before),
    );
    before.safe_to_automate = !gates.iter().any(|gate| gate.blocks_automation);

    let after_symbol = match (operation, new_symbol, phase) {
        (RefactorOperation::Move, _, VerificationPhase::Post) => Some(symbol),
        (RefactorOperation::Rename, Some(new_symbol), VerificationPhase::Post) => Some(new_symbol),
        _ => None,
    };

    let (target_kind, after) = match after_symbol {
        Some(after_symbol) => {
            let after_files =
                impact_report::workflow::collect_impact_reports(paths, dialect, after_symbol)?;
            let after_target_kind = resolve_target_kind(
                derive_refactor_target_kind(&after_files, after_symbol.as_str()),
                target_kind_hint,
            );
            let mut after = summarize_impact_reports(&after_files);
            let after_gates = application_refactor_plan_gates(
                application_operation,
                after_target_kind,
                &after,
                raw_refactor_risks(&after),
            );
            after.safe_to_automate = !after_gates.iter().any(|gate| gate.blocks_automation);
            (after_target_kind, Some(after))
        }
        _ => (before_target_kind, None),
    };
    let checks = application_refactor_verification_checks(
        RefactorVerificationRequest {
            operation: application_operation,
            phase: application_phase,
            symbol: symbol.as_str(),
            new_symbol: new_symbol.map(|symbol| symbol.as_str()),
            target_kind,
            before,
            after,
        },
        &gates,
    );
    let passed = checks.iter().all(|check| check.passed);
    Ok(RefactorVerification {
        operation: application_operation,
        phase: application_phase,
        symbol: symbol.as_str().to_owned(),
        new_symbol: new_symbol.map(|symbol| symbol.as_str().to_owned()),
        passed,
        checks,
        target_kind,
        before,
        after,
    })
}

fn resolve_target_kind(
    inferred: RefactorPlanTargetKind,
    hint: Option<RefactorPlanTargetKind>,
) -> RefactorPlanTargetKind {
    match inferred {
        RefactorPlanTargetKind::Unknown => hint.unwrap_or(RefactorPlanTargetKind::Unknown),
        _ => inferred,
    }
}
