use paredit_core_cli::CommandResult;

use crate::destructive_literal::cli::args::DestructiveLiteralReportArgs;
use crate::destructive_literal::cli::render::print_destructive_literal_report;
use crate::destructive_literal::usecase::{
    collect_destructive_literals, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn destructive_literal_report(args: DestructiveLiteralReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_destructive_literals(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_destructive_literal_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "destructive-literal-report policy failed: {message}"
        )));
    }

    Ok(())
}
