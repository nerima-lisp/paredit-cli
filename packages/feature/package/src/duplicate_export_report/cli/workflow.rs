use paredit_core_cli::CommandResult;

use crate::duplicate_export_report::cli::args::DuplicateExportReportArgs;
use crate::duplicate_export_report::cli::render::print_duplicate_export_report;
use crate::duplicate_export_report::usecase::{
    DuplicateExportPolicyOptions, collect_duplicate_exports, evaluate_duplicate_export_policy,
    summarize_duplicate_exports,
};
use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

pub fn duplicate_export_report(args: DuplicateExportReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _input| {
        collect_duplicate_exports(file, dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let (defpackage_counts, duplicate_lists): (Vec<_>, Vec<_>) =
        analysis.succeeded.into_iter().unzip();
    let defpackage_count: usize = defpackage_counts.into_iter().sum();
    let duplicates: Vec<_> = duplicate_lists.into_iter().flatten().collect();

    let summary = summarize_duplicate_exports(defpackage_count, duplicates);
    let policy = evaluate_duplicate_export_policy(
        DuplicateExportPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_export_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-export-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
