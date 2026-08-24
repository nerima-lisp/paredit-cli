use paredit_core_cli::{CliResult, CommandResult};

use crate::package_level_shadowing::cli::args::PackageLevelShadowingReportArgs;
use crate::package_level_shadowing::cli::render::print_package_level_shadowing_report;
use crate::package_level_shadowing::usecase::{
    PackageLevelShadowingPolicyOptions, collect_package_level_shadowing,
    evaluate_package_level_shadowing_policy, summarize_package_level_shadowing,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn package_level_shadowing_report(args: PackageLevelShadowingReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(collect_package_level_shadowing(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let mut scanned_form_count = 0;
    let mut violations = Vec::new();
    for (file_form_count, file_violations) in analysis.succeeded {
        scanned_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_package_level_shadowing(scanned_form_count, violations);
    let policy = evaluate_package_level_shadowing_policy(
        PackageLevelShadowingPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_package_level_shadowing_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "package-level-shadowing-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
