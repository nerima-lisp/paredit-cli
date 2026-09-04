use paredit_core_cli::CommandResult;

use crate::unused_export_report::cli::args::UnusedExportReportArgs;
use crate::unused_export_report::cli::render::print_unused_export_report;
use crate::unused_export_report::usecase::{
    UnusedExportPolicyOptions, analyze_unused_exports, collect_declared_exports,
    collect_referenced_symbols, evaluate_unused_export_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn unused_export_report(args: UnusedExportReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "unused-export-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
