use paredit_core_cli::CommandResult;

use crate::leftover_trace_call::cli::args::LeftoverTraceCallReportArgs;
use crate::leftover_trace_call::cli::render::print_leftover_trace_call_report;
use crate::leftover_trace_call::usecase::{
    LeftoverTraceCallPolicyOptions, collect_leftover_trace_call,
    evaluate_leftover_trace_call_policy, summarize_leftover_trace_call,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn leftover_trace_call_report(args: LeftoverTraceCallReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut scanned_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_leftover_trace_call(file, dialect, &tree)?;
        scanned_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_leftover_trace_call(scanned_form_count, violations);
    let policy = evaluate_leftover_trace_call_policy(
        LeftoverTraceCallPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_leftover_trace_call_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "leftover-trace-call-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
