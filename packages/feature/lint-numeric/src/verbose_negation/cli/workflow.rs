use paredit_core_cli::CommandResult;

use crate::verbose_negation::cli::args::VerboseNegationReportArgs;
use crate::verbose_negation::cli::render::print_verbose_negation_report;
use crate::verbose_negation::usecase::{
    VerboseNegationPolicyOptions, collect_verbose_negations, evaluate_verbose_negation_policy,
    summarize_verbose_negations,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn verbose_negation_report(args: VerboseNegationReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_verbose_negations(file, dialect, &tree)?;
        arithmetic_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_verbose_negations(arithmetic_form_count, violations);
    let policy = evaluate_verbose_negation_policy(
        VerboseNegationPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_verbose_negation_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "verbose-negation-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
