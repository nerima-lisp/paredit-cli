use paredit_core_cli::{CliResult, CommandResult};

use crate::unused_export_report::cli::args::UnusedExportReportArgs;
use crate::unused_export_report::cli::render::print_unused_export_report;
use crate::unused_export_report::usecase::{
    UnusedExportPolicyOptions, analyze_unused_exports, collect_declared_exports,
    collect_referenced_symbols, evaluate_unused_export_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn unused_export_report(args: UnusedExportReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let declared = collect_declared_exports(file, dialect, &tree)?;
        let referenced = collect_referenced_symbols(dialect, &tree)?;
        CliResult::Ok((declared, referenced))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let (declared_lists, referenced_lists): (Vec<_>, Vec<_>) =
        analysis.succeeded.into_iter().unzip();
    let declared: Vec<_> = declared_lists.into_iter().flatten().collect();
    let referenced: Vec<_> = referenced_lists.into_iter().flatten().collect();

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
