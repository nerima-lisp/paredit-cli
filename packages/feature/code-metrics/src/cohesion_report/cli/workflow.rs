use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::cohesion_report::cli::args::CohesionReportArgs;
use crate::cohesion_report::cli::render::print_isolated_report;
use crate::cohesion_report::usecase::{build_cohesion_report, evaluate_fail_on_isolated_policy};

pub fn cohesion_report(args: CohesionReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_cohesion_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_isolated_policy(args.fail_on_isolated, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_isolated_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect cohesion policy failed: {message}"
        )));
    }

    Ok(())
}
