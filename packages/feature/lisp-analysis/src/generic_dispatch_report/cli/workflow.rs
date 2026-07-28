use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::generic_dispatch_report::cli::args::GenericDispatchReportArgs;
use crate::generic_dispatch_report::cli::render::print_defect_report;
use crate::generic_dispatch_report::usecase::{
    build_generic_dispatch_report, evaluate_fail_on_defect_policy,
};

pub fn generic_dispatch_report(args: GenericDispatchReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_generic_dispatch_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_defect_policy(args.fail_on_defect, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_defect_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect generic-dispatch policy failed: {message}"
        )));
    }

    Ok(())
}
