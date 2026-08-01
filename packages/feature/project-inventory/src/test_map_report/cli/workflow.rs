use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::test_map_report::cli::args::TestMapReportArgs;
use crate::test_map_report::cli::render::print_untested_report;
use crate::test_map_report::usecase::{build_test_map_report, evaluate_fail_on_untested_policy};

pub fn test_map_report(args: TestMapReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_test_map_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_untested_policy(args.fail_on_untested, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_untested_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect test-map policy failed: {message}"
        )));
    }

    Ok(())
}
