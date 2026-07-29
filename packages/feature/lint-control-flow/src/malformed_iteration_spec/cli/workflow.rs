use paredit_core_cli::CommandResult;

use crate::malformed_iteration_spec::cli::args::MalformedIterationSpecReportArgs;
use crate::malformed_iteration_spec::cli::render::print_malformed_iteration_spec_report;
use crate::malformed_iteration_spec::usecase::{
    MalformedIterationSpecPolicyOptions, collect_malformed_iteration_specs,
    evaluate_malformed_iteration_spec_policy, summarize_malformed_iteration_specs,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn malformed_iteration_spec_report(args: MalformedIterationSpecReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut iteration_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_iteration_form_count, file_violations) =
            collect_malformed_iteration_specs(file, dialect, &tree)?;
        iteration_form_count += file_iteration_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_malformed_iteration_specs(iteration_form_count, violations);
    let policy = evaluate_malformed_iteration_spec_policy(
        MalformedIterationSpecPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_malformed_iteration_spec_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "malformed-iteration-spec-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
