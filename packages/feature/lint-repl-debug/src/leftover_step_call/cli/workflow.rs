use paredit_core_cli::{CliResult, CommandResult};

use crate::leftover_step_call::cli::args::LeftoverStepCallReportArgs;
use crate::leftover_step_call::cli::render::print_leftover_step_call_report;
use crate::leftover_step_call::usecase::{
    LeftoverStepCallPolicyOptions, collect_leftover_step_call, evaluate_leftover_step_call_policy,
    summarize_leftover_step_call,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn leftover_step_call_report(args: LeftoverStepCallReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(collect_leftover_step_call(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let (form_counts, violation_lists): (Vec<_>, Vec<_>) = analysis.succeeded.into_iter().unzip();
    let scanned_form_count: usize = form_counts.into_iter().sum();
    let violations: Vec<_> = violation_lists.into_iter().flatten().collect();

    let summary = summarize_leftover_step_call(scanned_form_count, violations);
    let policy = evaluate_leftover_step_call_policy(
        LeftoverStepCallPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_leftover_step_call_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "leftover-step-call-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
