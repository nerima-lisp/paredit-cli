use paredit_core_cli::CommandResult;

use crate::verbose_negation::cli::args::VerboseNegationReportArgs;
use crate::verbose_negation::cli::render::print_verbose_negation_report;
use crate::verbose_negation::usecase::{
    build_verbose_negation_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn verbose_negation_report(args: VerboseNegationReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_verbose_negation_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_verbose_negation_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "verbose-negation-report policy failed: {message}"
        )));
    }

    Ok(())
}
