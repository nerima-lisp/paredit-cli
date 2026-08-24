use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::shared::SemanticFile;
use crate::value_propagation_report::cli::args::ValuePropagationReportArgs;
use crate::value_propagation_report::cli::render::print_value_propagation_report;
use crate::value_propagation_report::usecase::{
    ValuePropagationPolicyOptions, build_value_propagation_report,
    evaluate_value_propagation_policy,
};

pub fn value_propagation_report(args: ValuePropagationReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_value_propagation_report(&SemanticFile::analyze(
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

    let policy = evaluate_value_propagation_policy(
        ValuePropagationPolicyOptions::new(args.min_coverage),
        &reports,
    );
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_value_propagation_report(&reports, &policy, args.blocked_only, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "value-propagation-report policy failed: {message}"
        )));
    }

    Ok(())
}
