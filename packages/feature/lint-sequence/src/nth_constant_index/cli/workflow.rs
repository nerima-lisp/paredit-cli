use paredit_core_cli::CommandResult;

use crate::nth_constant_index::cli::args::NthConstantIndexReportArgs;
use crate::nth_constant_index::cli::render::print_nth_constant_index_report;
use crate::nth_constant_index::usecase::{
    build_nth_constant_index_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nth_constant_index_report(args: NthConstantIndexReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_nth_constant_index_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_nth_constant_index_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nth-constant-index-report policy failed: {message}"
        )));
    }

    Ok(())
}
