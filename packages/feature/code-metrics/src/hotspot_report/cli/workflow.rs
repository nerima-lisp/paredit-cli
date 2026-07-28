use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::hotspot_report::cli::args::HotspotReportArgs;
use crate::hotspot_report::cli::render::print_hotspot_report;
use crate::hotspot_report::usecase::{
    build_hotspot_report, churn_target, evaluate_hotspot_policy, measure_churn,
};

pub fn hotspot_report(args: HotspotReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        // One `git log` per file. Batching would be faster and would also mean
        // a multi-repository run could not be answered per repository, which is
        // the case this is most useful in.
        let churn = measure_churn(&churn_target(file), &args.since);
        reports.push(build_hotspot_report(file, dialect, &tree, &churn));
    }

    let policy = evaluate_hotspot_policy(args.max_score, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_hotspot_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect hotspots policy failed: {message}"
        )));
    }

    Ok(())
}
