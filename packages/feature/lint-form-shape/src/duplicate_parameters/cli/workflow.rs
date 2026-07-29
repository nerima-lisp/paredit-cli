use paredit_core_cli::CommandResult;

use crate::duplicate_parameters::cli::args::DuplicateParameterReportArgs;
use crate::duplicate_parameters::cli::render::print_duplicate_parameter_report;
use crate::duplicate_parameters::usecase::{
    DuplicateParameterPolicyOptions, collect_duplicate_parameters,
    evaluate_duplicate_parameter_policy, summarize_duplicate_parameters,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn duplicate_parameter_report(args: DuplicateParameterReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut definition_count = 0;
    let mut duplicates = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_definition_count, file_duplicates) =
            collect_duplicate_parameters(file, dialect, &tree)?;
        definition_count += file_definition_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_parameters(definition_count, duplicates);
    let policy = evaluate_duplicate_parameter_policy(
        DuplicateParameterPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_parameter_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-parameter-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
