use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::external_system_report::cli::args::ExternalSystemReportArgs;
use crate::external_system_report::cli::render::print_external_report;
use crate::external_system_report::usecase::{
    build_external_system_report, evaluate_fail_on_external_policy,
};

pub fn external_system_report(args: ExternalSystemReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_external_system_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_external_policy(args.fail_on_external, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_external_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect external-systems policy failed: {message}"
        )));
    }

    Ok(())
}
