use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};
use paredit_core_cli::{CliResult, CommandResult};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use super::args::{DefinitionReportArgs, UnusedDefinitionReportArgs};
use super::render::{print_definition_report, print_unused_definition_report};
use crate::definition_report::usecase::{
    UnusedDefinitionPolicyOptions, build_definition_report, build_parsed_definition_file,
    collect_unused_definition_candidates, evaluate_unused_definition_policy,
};
use paredit_core_workspace::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files};

pub fn definition_report(args: DefinitionReportArgs) -> CliResult<()> {
    let files = expand_definition_report_inputs(&args.files, args.dialect)?;
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _input| {
        CliResult::Ok(build_definition_report(file.clone(), dialect, tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    print_definition_report(&analysis.succeeded, args.output)
}

pub fn unused_definition_report(args: UnusedDefinitionReportArgs) -> CommandResult {
    let files = expand_definition_report_inputs(&args.files, args.dialect)?;
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, input| {
        CliResult::Ok(build_parsed_definition_file(
            file.clone(),
            dialect,
            tree,
            &input.text,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let parsed = analysis.succeeded;

    let reports = collect_unused_definition_candidates(&parsed)?;
    let policy = evaluate_unused_definition_policy(
        UnusedDefinitionPolicyOptions::new(args.fail_on_unused, args.require_unused_definitions)
            .map_err(paredit_core_cli::gate::gate_failure)?,
        &reports,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_unused_definition_report(&reports, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "unused-definition-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}

fn expand_definition_report_inputs(
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
    let canonical = fs::canonicalize(&path).unwrap_or(path.clone());
    if seen.insert(canonical) {
        expanded.push(path);
    }
}
