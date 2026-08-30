use paredit_core_cli::{CliResult, CommandResult};

use crate::self_recursive_tail_call::cli::args::SelfRecursiveTailCallReportArgs;
use crate::self_recursive_tail_call::cli::render::print_self_recursive_tail_call_report;
use crate::self_recursive_tail_call::usecase::{
    SelfRecursiveTailCallPolicyOptions, collect_self_recursive_tail_call,
    evaluate_self_recursive_tail_call_policy, summarize_self_recursive_tail_call,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn self_recursive_tail_call_report(args: SelfRecursiveTailCallReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(collect_self_recursive_tail_call(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let mut scanned_form_count = 0;
    let mut violations = Vec::new();
    for (file_form_count, file_violations) in analysis.succeeded {
        scanned_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_self_recursive_tail_call(scanned_form_count, violations);
    let policy = evaluate_self_recursive_tail_call_policy(
        SelfRecursiveTailCallPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_self_recursive_tail_call_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "self-recursive-tail-call-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
