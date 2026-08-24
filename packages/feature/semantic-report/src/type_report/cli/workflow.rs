use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::shared::SemanticFile;
use crate::type_report::cli::args::TypeReportArgs;
use crate::type_report::cli::render::print_type_report;
use crate::type_report::usecase::{
    TypeReportPolicyOptions, build_type_report, evaluate_type_report_policy,
};

pub fn type_report(args: TypeReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_type_report(&SemanticFile::analyze(
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

    let policy = evaluate_type_report_policy(
        TypeReportPolicyOptions::new(args.fail_on_contradiction),
        &reports,
    );
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_type_report(&reports, &policy, args.contradictions_only, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "type-report policy failed: {message}"
        )));
    }

    Ok(())
}
