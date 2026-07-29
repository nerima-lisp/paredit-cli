use super::super::args::*;
use super::super::render::print_refactor_verification;
use super::super::types::verification::RefactorVerification;
use super::shared::derive_refactor_target_kind;
use crate::refactor::usecase::plan::RefactorPlanTargetKind;
use crate::refactor::usecase::plan::{
    RefactorOperation as ApplicationRefactorOperation, RefactorVerificationRequest,
    VerificationPhase as ApplicationVerificationPhase,
    refactor_plan_gates as application_refactor_plan_gates,
    refactor_verification_checks as application_refactor_verification_checks,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_edit::refactor_plan::{RefactorRiskLevel, RefactorVerificationCheck};
use paredit_core_syntax::sexpr::SymbolName;
use paredit_feature_project_analysis::impact_report::cli as impact_report;
use paredit_feature_project_analysis::impact_report::usecase::{
    raw_refactor_risks, summarize_impact_reports,
};
use paredit_feature_rename::macro_construction::find_macro_construction_sites;
use std::path::PathBuf;

pub fn verify_refactor(args: VerifyRefactorArgs) -> CliResult<()> {
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

pub fn build_refactor_verification(
    paths: &[PathBuf],
    dialect: Option<DialectArg>,
    symbol: &SymbolName,
    new_symbol: Option<&SymbolName>,
    operation: RefactorOperation,
    phase: VerificationPhase,
    target_kind_hint: Option<RefactorPlanTargetKind>,
) -> CliResult<RefactorVerification> {
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
    let mut checks = application_refactor_verification_checks(
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
    checks.push(macro_construction_check(paths, dialect, symbol)?);
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

/// Reports the names this refactor cannot reach because they do not exist
/// until macro-expansion time.
///
/// A rename in this tool is syntactic: it rewrites the atoms whose text is the
/// symbol. `(intern "HANDLE-CLICK")` is not one of those atoms, and a rename
/// that reported "2 occurrences renamed" while leaving it behind would have
/// said something false. This check makes the gap part of the verification
/// rather than something the caller discovers at run time.
///
/// It is a `Warning`, not an `Error`. A construction site is not proof the
/// rename is wrong — most of them name something else entirely — and blocking
/// every rename in a file that calls `intern` once would make the check the
/// first thing anyone turned off.
fn macro_construction_check(
    paths: &[PathBuf],
    dialect: Option<DialectArg>,
    symbol: &SymbolName,
) -> CliResult<RefactorVerificationCheck> {
    let mut sites = Vec::new();
    for path in paths {
        let (input, _, tree) = read_input_dialect_and_tree(Some(path.clone()), dialect)?;
        sites.extend(
            find_macro_construction_sites(&tree, &input.text, Some(symbol.as_str()))
                .into_iter()
                .map(|site| (path.clone(), site)),
        );
    }

    let message = if sites.is_empty() {
        "no symbol is constructed from text that a syntactic rename would miss".to_owned()
    } else {
        let described = sites
            .iter()
            .take(MACRO_SITES_REPORTED)
            .map(|(path, site)| {
                format!(
                    "{}:{} {} ({})",
                    path.display(),
                    site.line,
                    site.operator,
                    site.kind.label()
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let remainder = sites.len().saturating_sub(MACRO_SITES_REPORTED);
        format!(
            "{} site(s) construct a symbol from text a syntactic rename cannot rewrite: {described}{}",
            sites.len(),
            if remainder == 0 {
                String::new()
            } else {
                format!(" (and {remainder} more)")
            }
        )
    };

    Ok(RefactorVerificationCheck {
        code: "macro-constructed-symbols",
        level: RefactorRiskLevel::Warning,
        passed: sites.is_empty(),
        message,
        count: sites.len(),
    })
}

/// How many construction sites the check names before summarising the rest.
///
/// A one-line check message that lists forty sites is a check nobody reads.
const MACRO_SITES_REPORTED: usize = 5;

fn resolve_target_kind(
    inferred: RefactorPlanTargetKind,
    hint: Option<RefactorPlanTargetKind>,
) -> RefactorPlanTargetKind {
    match inferred {
        RefactorPlanTargetKind::Unknown => hint.unwrap_or(RefactorPlanTargetKind::Unknown),
        _ => inferred,
    }
}
