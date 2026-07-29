use paredit_core_cli::CommandResult;

use crate::negated_when_unless::cli::args::NegatedWhenUnlessReportArgs;
use crate::negated_when_unless::cli::render::print_negated_when_unless_report;
use crate::negated_when_unless::usecase::{
    NegatedWhenUnlessPolicyOptions, collect_negated_when_unless,
    evaluate_negated_when_unless_policy, summarize_negated_when_unless,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn negated_when_unless_report(args: NegatedWhenUnlessReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut conditional_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_negated_when_unless(file, dialect, &tree)?;
        conditional_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_negated_when_unless(conditional_form_count, violations);
    let policy = evaluate_negated_when_unless_policy(
        NegatedWhenUnlessPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_negated_when_unless_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "negated-when-unless-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
