use super::super::*;
use super::args::ImpactReportArgs;
use super::render::print_impact_report;
use crate::application::usecase::impact_report::{
    ImpactReportFile, ImpactReportPolicyOptions, ImpactReportSource, build_impact_reports,
    evaluate_impact_report_policy, impact_risks, impact_status_counts, summarize_impact_reports,
};

pub(in crate::presentation::cli) fn impact_report(args: ImpactReportArgs) -> Result<()> {
    let reports = collect_impact_reports(&args.files, args.dialect, &args.symbol)?;
    let summary = summarize_impact_reports(&reports);
    let by_status = impact_status_counts(&reports);
    let risks = impact_risks(
        summary.definition_count,
        summary.inbound_edge_count,
        summary.non_call_reference_count,
        &by_status,
    );
    let risk_level = risks
        .iter()
        .map(|risk| risk.level)
        .max()
        .unwrap_or(ApplicationImpactRiskLevel::Info);
    let policy = evaluate_impact_report_policy(
        ImpactReportPolicyOptions::new(
            args.fail_on_risk_level.map(Into::into),
            args.require_definitions,
            args.require_references,
            args.require_calls,
        )
        .map_err(anyhow::Error::msg)?,
        &summary,
        risk_level,
    );

    print_impact_report(&reports, &args.symbol, &policy, args.output)?;
    if !policy.passed {
        let policy_message = policy.violations.join("; ");
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "impact-report policy failed: {policy_message}"
        )));
    }
    Ok(())
}

pub(in crate::presentation::cli) fn collect_impact_reports(
    files: &[PathBuf],
    dialect_override: Option<DialectArg>,
    symbol: &SymbolName,
) -> Result<Vec<ImpactReportFile>> {
    let mut sources = Vec::with_capacity(files.len());

    for file in files {
        let (_input, dialect, tree) =
            read_input_dialect_and_tree(Some(file.clone()), dialect_override)?;
        sources.push(ImpactReportSource {
            path: file.clone(),
            dialect,
            tree,
        });
    }

    build_impact_reports(sources, symbol)
}
