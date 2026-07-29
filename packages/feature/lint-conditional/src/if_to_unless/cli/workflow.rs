use paredit_core_cli::CommandResult;

use crate::if_to_unless::cli::args::IfToUnlessReportArgs;
use crate::if_to_unless::cli::render::print_if_to_unless_report;
use crate::if_to_unless::usecase::{
    IfToUnlessPolicyOptions, collect_if_to_unless, evaluate_if_to_unless_policy,
    summarize_if_to_unless,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn if_to_unless_report(args: IfToUnlessReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_if_to_unless(file, dialect, &tree)?;
        if_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_if_to_unless(if_form_count, violations);
    let policy = evaluate_if_to_unless_policy(
        IfToUnlessPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_if_to_unless_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "if-to-unless-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
