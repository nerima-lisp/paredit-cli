use paredit_core_cli::CommandResult;

use crate::redefinition_report::cli::args::RedefinitionReportArgs;
use crate::redefinition_report::cli::render::print_redefinition_report;
use crate::redefinition_report::usecase::{
    RedefinitionPolicyOptions, analyze_redefinitions, collect_declared_definitions,
    evaluate_redefinition_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn redefinition_report(args: RedefinitionReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        collect_declared_definitions(file, dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let declared: Vec<_> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_redefinitions(&declared);
    let policy = evaluate_redefinition_policy(
        RedefinitionPolicyOptions::new(args.fail_on_redefinition),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redefinition_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redefinition-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
