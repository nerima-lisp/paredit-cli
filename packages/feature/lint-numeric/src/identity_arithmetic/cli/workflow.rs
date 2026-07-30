use paredit_core_cli::CommandResult;

use crate::identity_arithmetic::cli::args::IdentityArithmeticReportArgs;
use crate::identity_arithmetic::cli::render::print_identity_arithmetic_report;
use crate::identity_arithmetic::usecase::{
    build_identity_arithmetic_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn identity_arithmetic_report(args: IdentityArithmeticReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_identity_arithmetic_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_identity_arithmetic_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "identity-arithmetic-report policy failed: {message}"
        )));
    }

    Ok(())
}
