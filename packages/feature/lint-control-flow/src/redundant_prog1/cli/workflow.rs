use paredit_core_cli::CommandResult;

use crate::redundant_prog1::cli::args::RedundantProg1ReportArgs;
use crate::redundant_prog1::cli::render::print_redundant_prog1_report;
use crate::redundant_prog1::usecase::{
    RedundantProg1PolicyOptions, collect_redundant_prog1s, evaluate_redundant_prog1_policy,
    summarize_redundant_prog1s,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_prog1_report(args: RedundantProg1ReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut prog1_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_prog1s(file, dialect, &tree)?;
        prog1_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_prog1s(prog1_form_count, violations);
    let policy = evaluate_redundant_prog1_policy(
        RedundantProg1PolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_prog1_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-prog1-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
