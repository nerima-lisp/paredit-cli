use paredit_core_cli::CommandResult;

use crate::prog2_to_progn::cli::args::Prog2ToPrognReportArgs;
use crate::prog2_to_progn::cli::render::print_prog2_to_progn_report;
use crate::prog2_to_progn::usecase::{
    build_prog2_to_progn_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn prog2_to_progn_report(args: Prog2ToPrognReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_prog2_to_progn_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_prog2_to_progn_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "prog2-to-progn-report policy failed: {message}"
        )));
    }

    Ok(())
}
