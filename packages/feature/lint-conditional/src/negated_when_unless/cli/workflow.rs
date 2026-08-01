use paredit_core_cli::CommandResult;

use crate::negated_when_unless::cli::args::NegatedWhenUnlessReportArgs;
use crate::negated_when_unless::cli::render::print_negated_when_unless_report;
use crate::negated_when_unless::usecase::{
    build_negated_when_unless_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn negated_when_unless_report(args: NegatedWhenUnlessReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_negated_when_unless_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_negated_when_unless_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "negated-when-unless-report policy failed: {message}"
        )));
    }

    Ok(())
}
