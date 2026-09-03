use paredit_core_cli::CommandResult;

use crate::asdf_self_referential_depends_on::cli::args::AsdfSelfReferentialDependsOnReportArgs;
use crate::asdf_self_referential_depends_on::cli::render::print_asdf_self_referential_depends_on_report;
use crate::asdf_self_referential_depends_on::usecase::{
    build_asdf_self_referential_depends_on_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn asdf_self_referential_depends_on_report(
    args: AsdfSelfReferentialDependsOnReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_asdf_self_referential_depends_on_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_asdf_self_referential_depends_on_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "asdf-self-referential-depends-on-report policy failed: {message}"
        )));
    }

    Ok(())
}
