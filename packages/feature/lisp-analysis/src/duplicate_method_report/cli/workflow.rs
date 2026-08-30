use paredit_core_cli::CommandResult;

use crate::duplicate_method_report::cli::args::DuplicateMethodReportArgs;
use crate::duplicate_method_report::cli::render::print_duplicate_method_report;
use crate::duplicate_method_report::usecase::{
    DuplicateMethodPolicyOptions, analyze_duplicate_methods, collect_declared_methods,
    evaluate_duplicate_method_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn duplicate_method_report(args: DuplicateMethodReportArgs) -> CommandResult {
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _input| {
        collect_declared_methods(file, dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let declared: Vec<_> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_duplicate_methods(&declared);
    let policy = evaluate_duplicate_method_policy(
        DuplicateMethodPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_method_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-method-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
