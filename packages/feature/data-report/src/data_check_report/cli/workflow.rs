use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::data_check_report::cli::args::DataCheckReportArgs;
use crate::data_check_report::cli::render::print_data_check_report;
use crate::data_check_report::usecase::{
    build_data_check_report, detect_data_format, evaluate_fail_on_finding_policy,
};

pub fn data_check_report(args: DataCheckReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        // An explicit `--format` always wins; otherwise the file's path and
        // content decide which detectors run on top of the baseline checks.
        let format = args
            .format
            .map_or_else(|| detect_data_format(file, dialect, &tree), Into::into);
        reports.push(build_data_check_report(file, dialect, &tree, format));
    }

    let policy = evaluate_fail_on_finding_policy(args.fail_on_finding, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_data_check_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect data-check policy failed: {message}"
        )));
    }

    Ok(())
}
