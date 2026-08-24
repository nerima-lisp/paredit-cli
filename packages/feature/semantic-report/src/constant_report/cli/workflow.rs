use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::constant_report::cli::args::ConstantReportArgs;
use crate::constant_report::cli::render::print_constant_report;
use crate::constant_report::usecase::{
    ConstantReportPolicyOptions, build_constant_report, evaluate_constant_report_policy,
};
use crate::shared::SemanticFile;

pub fn constant_report(args: ConstantReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_constant_report(&SemanticFile::analyze(
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

    let policy = evaluate_constant_report_policy(
        ConstantReportPolicyOptions::new(args.fail_on_foldable),
        &reports,
    );
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_constant_report(&reports, &policy, args.min_saved_bytes, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "constant-report policy failed: {message}"
        )));
    }

    Ok(())
}
