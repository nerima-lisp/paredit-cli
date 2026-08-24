use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::hotspot_report::cli::args::HotspotReportArgs;
use crate::hotspot_report::cli::render::print_hotspot_report;
use crate::hotspot_report::usecase::{
    build_hotspot_report, churn_target, evaluate_hotspot_policy, measure_churn,
};

pub fn hotspot_report(args: HotspotReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        // One `git log` per file. Batching would be faster and would also mean
        // a multi-repository run could not be answered per repository, which is
        // the case this is most useful in.
        let churn = measure_churn(&churn_target(file), &args.since);
        CliResult::Ok(build_hotspot_report(file, dialect, tree, &churn))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_hotspot_policy(args.max_score, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_hotspot_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect hotspots policy failed: {message}"
        )));
    }

    Ok(())
}
