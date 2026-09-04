use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::restart_report::cli::args::RestartReportArgs;
use crate::restart_report::cli::render::print_unpaired_report;
use crate::restart_report::usecase::{build_restart_report, evaluate_fail_on_unpaired_policy};

pub fn restart_report(args: RestartReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_restart_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_unpaired_policy(args.fail_on_unpaired, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_unpaired_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect restarts policy failed: {message}"
        )));
    }

    Ok(())
}
