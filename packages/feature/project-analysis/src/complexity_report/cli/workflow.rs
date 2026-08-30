use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};
use paredit_core_cli::{CliResult, CommandResult};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use super::args::ComplexityReportArgs;
use super::render::print_complexity_report;
use crate::complexity_report::usecase::{
    ComplexityReportPolicyOptions, build_complexity_report, evaluate_complexity_report_policy,
};
use paredit_core_workspace::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files};

pub fn complexity_report(args: ComplexityReportArgs) -> CommandResult {
    let files = expand_complexity_report_inputs(&args.files, args.dialect)?;

    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        build_complexity_report(file.to_path_buf(), dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_complexity_report_policy(
        ComplexityReportPolicyOptions::new(args.fail_on_max_depth),
        &reports,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_complexity_report(&reports, &policy, args.top, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "complexity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}

fn expand_complexity_report_inputs(
    files: &[PathBuf],
    dialect: Option<paredit_core_cli::args::DialectArg>,
) -> CliResult<Vec<PathBuf>> {
    let mut expanded = Vec::new();
    let mut seen = BTreeSet::new();

    for file in files {
        if file.is_dir() {
            let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
                roots: vec![file.clone()],
                include_unknown: dialect.is_some(),
                include_hidden: false,
                include_generated: false,
                max_depth: None,
                exclude: Vec::new(),
                ..WorkspaceDiscoveryOptions::default()
            })?;

            for discovered in discovery.into_files() {
                push_unique(&mut expanded, &mut seen, discovered);
            }
        } else {
            push_unique(&mut expanded, &mut seen, file.clone());
        }
    }

    Ok(expanded)
}

fn push_unique(expanded: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: PathBuf) {
    let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert(canonical) {
        expanded.push(path);
    }
}
