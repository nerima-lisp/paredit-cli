use paredit_core_cli::CommandResult;

use crate::redundant_funcall::cli::args::RedundantFuncallReportArgs;
use crate::redundant_funcall::cli::render::print_redundant_funcall_report;
use crate::redundant_funcall::usecase::{
    RedundantFuncallPolicyOptions, collect_redundant_funcalls, evaluate_redundant_funcall_policy,
    summarize_redundant_funcalls,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_funcall_report(args: RedundantFuncallReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut funcall_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_funcalls(file, dialect, &tree)?;
        funcall_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_funcalls(funcall_form_count, violations);
    let policy = evaluate_redundant_funcall_policy(
        RedundantFuncallPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_funcall_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-funcall-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
