mod build;
mod failure;
mod write;

use super::super::super::*;
use super::super::args::*;
use super::super::render::{print_refactor_preview, refactor_preview_manifest_json};
use super::super::types::plan::WorkspaceRefactorPlanDiscovery;
use super::workspace::discover_workspace_refactor_scope;

pub(in crate::presentation::cli::refactor::workflow) use build::{
    BuildRefactorPreviewRequest, build_refactor_preview,
};
pub(in crate::presentation::cli::refactor::workflow) use failure::finish_refactor_preview_failure;
pub(in crate::presentation::cli::refactor::workflow) use write::write_refactor_preview;

pub(in crate::presentation::cli) fn refactor_preview(args: RefactorPreviewArgs) -> Result<()> {
    emit_refactor_preview(RefactorPreviewEmission {
        paths: &args.files,
        dialect: args.dialect,
        from: &args.from,
        to: &args.to,
        mode: args.mode,
        max_preview_bytes: args.max_preview_bytes,
        write: args.write,
        policy_options: preview_policy_options(
            args.fail_on_no_change,
            args.fail_on_parse_error,
            args.fail_on_target_conflict,
            args.require_changed_files,
            args.require_definitions,
            args.require_edits,
        )?,
        workspace: None,
        manifest_out: args.manifest_out,
        output: args.output,
        failure_label: "refactor preview",
    })
}

pub(in crate::presentation::cli) fn workspace_refactor_preview(
    args: WorkspaceRefactorPreviewArgs,
) -> Result<()> {
    let workspace = discover_workspace_refactor_scope(WorkspaceDiscoveryOptions {
        roots: args.roots.clone(),
        include_unknown: args.include_unknown,
        include_hidden: args.include_hidden,
        include_generated: args.include_generated,
        max_depth: args.max_depth,
        exclude: Vec::new(),
    })?;

    emit_refactor_preview(RefactorPreviewEmission {
        paths: &workspace.paths,
        dialect: None,
        from: &args.from,
        to: &args.to,
        mode: args.mode,
        max_preview_bytes: args.max_preview_bytes,
        write: args.write,
        policy_options: preview_policy_options(
            args.fail_on_no_change,
            args.fail_on_parse_error,
            args.fail_on_target_conflict,
            args.require_changed_files,
            args.require_definitions,
            args.require_edits,
        )?,
        workspace: Some(workspace.workspace),
        manifest_out: args.manifest_out,
        output: args.output,
        failure_label: "refactor workspace-preview",
    })
}

fn preview_policy_options(
    fail_on_no_change: bool,
    fail_on_parse_error: bool,
    fail_on_target_conflict: bool,
    require_changed_files: Option<usize>,
    require_definitions: Option<usize>,
    require_edits: Option<usize>,
) -> Result<DomainRefactorPreviewPolicyOptions> {
    DomainRefactorPreviewPolicyOptions::new(
        fail_on_no_change,
        fail_on_parse_error,
        fail_on_target_conflict,
        require_changed_files,
        require_definitions,
        require_edits,
    )
    .map_err(anyhow::Error::msg)
}

struct RefactorPreviewEmission<'a> {
    paths: &'a [PathBuf],
    dialect: Option<DialectArg>,
    from: &'a SymbolName,
    to: &'a SymbolName,
    mode: RefactorPreviewMode,
    max_preview_bytes: usize,
    write: bool,
    policy_options: DomainRefactorPreviewPolicyOptions,
    workspace: Option<WorkspaceRefactorPlanDiscovery>,
    manifest_out: Option<PathBuf>,
    output: OutputFormat,
    failure_label: &'static str,
}

fn emit_refactor_preview(request: RefactorPreviewEmission<'_>) -> Result<()> {
    let RefactorPreviewEmission {
        paths,
        dialect,
        from,
        to,
        mode,
        max_preview_bytes,
        write,
        policy_options,
        workspace,
        manifest_out,
        output,
        failure_label,
    } = request;
    let mut preview = build_refactor_preview(BuildRefactorPreviewRequest {
        paths,
        dialect,
        from,
        to,
        mode,
        max_preview_bytes,
        write,
        policy_options,
        workspace,
    })?;
    let policy_passed = preview.policy.passed();
    let policy_message = preview.policy.violations().join("; ");
    let write_parse_refused = write && !preview.summary.all_outputs_parse();

    if policy_passed && !write_parse_refused {
        write_refactor_preview(&mut preview)?;
    }

    match manifest_out {
        Some(manifest_path) => write_manifest_and_print_summary(&preview, &manifest_path, output)?,
        None => print_refactor_preview(&preview, output)?,
    }
    finish_refactor_preview_failure(
        failure_label,
        policy_passed,
        &policy_message,
        write_parse_refused,
    )
}

fn write_manifest_and_print_summary(
    preview: &crate::presentation::cli::refactor::types::preview::RefactorPreview,
    manifest_path: &FsPath,
    output: OutputFormat,
) -> Result<()> {
    let manifest_text = format!(
        "{}\n",
        serde_json::to_string_pretty(&refactor_preview_manifest_json(preview))?
    );
    let manifest_hash = stable_text_hash(&manifest_text);
    write_artifact_with_rollback(manifest_path.to_path_buf(), manifest_text)
        .with_context(|| format!("failed to write manifest {}", manifest_path.display()))?;

    match output {
        OutputFormat::Text => {
            println!("manifest_path\t{}", safe_text!(manifest_path.display()));
            println!("manifest_hash\t{manifest_hash}");
            println!(
                "changed_file_count\t{}",
                preview.summary.changed_file_count()
            );
            println!("edit_count\t{}", preview.summary.edit_count());
            println!("policy_passed\t{}", preview.policy.passed());
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "manifest_path": manifest_path.display().to_string(),
                "manifest_hash": manifest_hash,
                "summary": {
                    "file_count": preview.summary.file_count(),
                    "changed_file_count": preview.summary.changed_file_count(),
                    "edit_count": preview.summary.edit_count(),
                    "all_outputs_parse": preview.summary.all_outputs_parse(),
                },
                "policy": {
                    "passed": preview.policy.passed(),
                    "violations": preview.policy.violations(),
                },
                "next_actions": [
                    format!(
                        "paredit refactor status --manifest {} --root . --output json",
                        manifest_path.display()
                    ),
                    format!(
                        "paredit refactor apply --manifest {} --expect-manifest-hash {manifest_hash} --root . --write --output json",
                        manifest_path.display()
                    ),
                ],
            }))?
        ),
    }
    Ok(())
}
