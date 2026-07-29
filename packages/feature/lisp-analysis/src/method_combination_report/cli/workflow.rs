use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::method_combination_report::cli::args::MethodCombinationReportArgs;
use crate::method_combination_report::cli::render::print_orphaned_report;
use crate::method_combination_report::usecase::{
    build_method_combination_report, evaluate_fail_on_orphaned_policy,
};

pub fn method_combination_report(args: MethodCombinationReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_method_combination_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_orphaned_policy(args.fail_on_orphaned, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_orphaned_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect method-combination policy failed: {message}"
        )));
    }

    Ok(())
}
