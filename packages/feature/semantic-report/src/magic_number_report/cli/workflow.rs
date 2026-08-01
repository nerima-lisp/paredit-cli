use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::magic_number_report::cli::args::MagicNumberReportArgs;
use crate::magic_number_report::cli::render::print_magic_number_report;
use crate::magic_number_report::usecase::{
    build_magic_number_report, evaluate_fail_on_magic_number_policy,
};

pub fn magic_number_report(args: MagicNumberReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_magic_number_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_magic_number_policy(args.fail_on_magic_number, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_magic_number_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect magic-numbers policy failed: {message}"
        )));
    }

    Ok(())
}
