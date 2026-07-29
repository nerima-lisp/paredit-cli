use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use super::super::*;
use super::args::ShadowedBindingReportArgs;
use super::render::print_shadowed_binding_report;
use crate::application::usecase::shadowed_binding_report::{
    ShadowedBindingPolicyOptions, build_shadowed_binding_report, evaluate_shadowed_binding_policy,
};
use crate::infrastructure::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files};

pub(in crate::presentation::cli) fn shadowed_binding_report(
    args: ShadowedBindingReportArgs,
) -> CommandResult {
    let files = expand_shadowed_binding_report_inputs(&args.files, args.dialect)?;
    let mut reports = Vec::with_capacity(files.len());

    for file in &files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_shadowed_binding_report(
            file.clone(),
            dialect,
            &input.text,
            &tree,
        )?);
    }

    let policy = evaluate_shadowed_binding_policy(
        ShadowedBindingPolicyOptions::new(args.fail_on_shadowed),
        &reports,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_shadowed_binding_report(&reports, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "shadowed-binding-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}

fn expand_shadowed_binding_report_inputs(
    files: &[PathBuf],
    dialect: Option<super::super::DialectArg>,
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
