use paredit_core_cli::CommandResult;

use crate::if_arity::cli::args::IfArityReportArgs;
use crate::if_arity::cli::render::print_if_arity_report;
use crate::if_arity::usecase::{
    IfArityPolicyOptions, collect_if_arity_violations, evaluate_if_arity_policy, summarize_if_arity,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn if_arity_report(args: IfArityReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_if_form_count, file_violations) =
            collect_if_arity_violations(file, dialect, &tree)?;
        if_form_count += file_if_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_if_arity(if_form_count, violations);
    let policy =
        evaluate_if_arity_policy(IfArityPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_if_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "if-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
