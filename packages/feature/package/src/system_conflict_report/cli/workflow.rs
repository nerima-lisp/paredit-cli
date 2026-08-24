use paredit_core_cli::{CliResult, CommandResult};

use crate::system_conflict_report::cli::args::SystemConflictReportArgs;
use crate::system_conflict_report::cli::render::print_system_conflict_report;
use crate::system_conflict_report::usecase::{
    SystemConflictPolicyOptions, analyze_system_conflicts, collect_declared_systems,
    evaluate_system_conflict_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn system_conflict_report(args: SystemConflictReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(collect_declared_systems(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let declared: Vec<_> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_system_conflicts(&declared);
    let policy = evaluate_system_conflict_policy(
        SystemConflictPolicyOptions::new(args.fail_on_conflict),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_system_conflict_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "system-conflict-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
