use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::use_widening_report::cli::args::UseWideningReportArgs;
use crate::use_widening_report::cli::render::print_use_widening_report;
use crate::use_widening_report::usecase::{build_use_widening_report, evaluate_fail_on_use_policy};

pub fn use_widening_report(args: UseWideningReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_use_widening_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_use_policy(args.fail_on_use, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_use_widening_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect use-widening policy failed: {message}"
        )));
    }

    Ok(())
}
