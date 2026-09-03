use paredit_core_cli::CommandResult;

use crate::when_unless_implicit_nil_misused::cli::args::WhenUnlessImplicitNilMisusedReportArgs;
use crate::when_unless_implicit_nil_misused::cli::render::print_when_unless_implicit_nil_misused_report;
use crate::when_unless_implicit_nil_misused::usecase::{
    build_when_unless_implicit_nil_misused_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn when_unless_implicit_nil_misused_report(
    args: WhenUnlessImplicitNilMisusedReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_when_unless_implicit_nil_misused_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_when_unless_implicit_nil_misused_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "when-unless-implicit-nil-misused-report policy failed: {message}"
        )));
    }

    Ok(())
}
