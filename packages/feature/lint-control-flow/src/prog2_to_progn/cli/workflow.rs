use paredit_core_cli::{CliResult, CommandResult};

use crate::prog2_to_progn::cli::args::Prog2ToPrognReportArgs;
use crate::prog2_to_progn::cli::render::print_prog2_to_progn_report;
use crate::prog2_to_progn::usecase::{
    build_prog2_to_progn_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn prog2_to_progn_report(args: Prog2ToPrognReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_prog2_to_progn_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_prog2_to_progn_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "prog2-to-progn-report policy failed: {message}"
        )));
    }

    Ok(())
}
