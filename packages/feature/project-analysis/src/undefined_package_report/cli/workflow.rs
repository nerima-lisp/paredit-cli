use paredit_core_cli::CommandResult;

use crate::error::ProjectAnalysisResult;
use crate::undefined_package_report::cli::args::UndefinedPackageReportArgs;
use crate::undefined_package_report::cli::render::print_undefined_package_report;
use crate::undefined_package_report::usecase::{
    UndefinedPackagePolicyOptions, analyze_undefined_packages, collect_declared_package_names,
    collect_in_package_references, evaluate_undefined_package_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn undefined_package_report(args: UndefinedPackageReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        let declared = collect_declared_package_names(dialect, tree)?;
        let referenced = collect_in_package_references(file, dialect, tree)?;
        ProjectAnalysisResult::Ok((declared, referenced))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let (declared, referenced): (Vec<_>, Vec<_>) = analysis.succeeded.into_iter().unzip();
    let declared: Vec<_> = declared.into_iter().flatten().collect();
    let referenced: Vec<_> = referenced.into_iter().flatten().collect();

    let summary = analyze_undefined_packages(&declared, &referenced);
    let policy = evaluate_undefined_package_policy(
        UndefinedPackagePolicyOptions::new(args.fail_on_undefined),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_undefined_package_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "undefined-package-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
