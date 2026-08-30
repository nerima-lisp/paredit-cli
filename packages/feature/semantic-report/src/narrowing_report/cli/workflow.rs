use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::narrowing_report::cli::args::NarrowingReportArgs;
use crate::narrowing_report::cli::render::print_narrowing_report;
use crate::narrowing_report::usecase::{
    NarrowingPolicyOptions, build_narrowing_report, evaluate_narrowing_policy,
};
use crate::shared::SemanticFile;

pub fn narrowing_report(args: NarrowingReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_narrowing_report(&SemanticFile::analyze(
            file,
            dialect,
            tree.clone(),
        )))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy =
        evaluate_narrowing_policy(NarrowingPolicyOptions::new(args.fail_on_none), &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_narrowing_report(&reports, &policy, args.binding.as_deref(), args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "narrowing-report policy failed: {message}"
        )));
    }

    Ok(())
}
