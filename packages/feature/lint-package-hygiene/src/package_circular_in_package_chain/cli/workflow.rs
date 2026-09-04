use paredit_core_cli::CommandResult;

use crate::package_circular_in_package_chain::cli::args::CircularInPackageChainReportArgs;
use crate::package_circular_in_package_chain::cli::render::print_circular_in_package_chain_report;
use crate::package_circular_in_package_chain::usecase::{
    build_circular_in_package_chain_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn circular_in_package_chain_report(args: CircularInPackageChainReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_circular_in_package_chain_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_circular_in_package_chain_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "package-circular-in-package-chain-report policy failed: {message}"
        )));
    }

    Ok(())
}
