use anyhow::Result;

use crate::application::usecase::unused_export_report::{
    UnusedExportPolicyOptions, analyze_unused_exports, collect_declared_exports,
    collect_referenced_symbols, evaluate_unused_export_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::unused_export_report::args::UnusedExportReportArgs;
use crate::presentation::cli::unused_export_report::render::print_unused_export_report;

pub(in crate::presentation::cli) fn unused_export_report(
    args: UnusedExportReportArgs,
) -> Result<()> {
    let mut declared = Vec::new();
    let mut referenced = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_exports(file, dialect, &tree)?);
        referenced.extend(collect_referenced_symbols(dialect, &tree)?);
    }

    let summary = analyze_unused_exports(&declared, &referenced);
    let policy = evaluate_unused_export_policy(
        UnusedExportPolicyOptions::new(args.fail_on_unused),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_unused_export_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "unused-export-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
