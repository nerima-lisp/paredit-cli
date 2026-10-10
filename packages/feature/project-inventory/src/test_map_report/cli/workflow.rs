use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::test_map_report::cli::args::TestMapReportArgs;
use crate::test_map_report::cli::render::print_untested_report;
use crate::test_map_report::usecase::{build_test_map_reports, evaluate_fail_on_untested_policy};

pub fn test_map_report(args: TestMapReportArgs) -> CommandResult {
    let mut files = expand_input_files(&args.files, args.dialect)?;
    files.sort();

    let mut inputs = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        inputs.push((file.as_path(), dialect, tree));
    }
    let documents = inputs
        .iter()
        .map(|(path, dialect, tree)| (*path, *dialect, tree))
        .collect::<Vec<_>>();
    let reports = build_test_map_reports(&documents);

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
