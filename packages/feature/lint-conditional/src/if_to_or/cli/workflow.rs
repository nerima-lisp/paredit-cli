use paredit_core_cli::CommandResult;

use crate::if_to_or::cli::args::IfToOrReportArgs;
use crate::if_to_or::cli::render::print_if_to_or_report;
use crate::if_to_or::usecase::{
    IfToOrPolicyOptions, collect_if_to_ors, evaluate_if_to_or_policy, summarize_if_to_ors,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn if_to_or_report(args: IfToOrReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_if_to_ors(file, dialect, &tree)?;
        if_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_if_to_ors(if_form_count, violations);
    let policy =
        evaluate_if_to_or_policy(IfToOrPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_if_to_or_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "if-to-or-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
