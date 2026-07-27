use anyhow::Result;

use crate::application::usecase::rename::{self as rename_usecase, RenameTarget};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

pub(super) fn rename_target(path: Option<Path>, at: Option<usize>) -> Result<RenameTarget> {
    match (path, at) {
        (Some(path), None) => Ok(RenameTarget::Path(path)),
        (None, Some(offset)) => Ok(RenameTarget::Offset(offset)),
        (None, None) => anyhow::bail!("target required: pass --path or --at"),
        (Some(_), Some(_)) => anyhow::bail!("pass only one of --path or --at"),
    }
}

pub(in crate::presentation::cli) fn collect_callable_definition_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<rename_usecase::RenameFunctionOccurrence>> {
    rename_usecase::collect_callable_definition_renames(tree, dialect, from, to)
}

pub(in crate::presentation::cli) fn collect_function_call_head_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<rename_usecase::RenameFunctionOccurrence>> {
    rename_usecase::collect_function_call_head_renames(tree, dialect, from, to)
}

pub(super) fn ensure_rename_changed(
    fail_on_no_change: bool,
    changed: bool,
    command: &str,
) -> Result<()> {
    if fail_on_no_change && !changed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "{command} policy failed: no occurrence changed"
        )));
    }
    Ok(())
}

/// Evaluates the shared --fail-on-no-change / --require-calls policy used by
/// the wrap/replace/unwrap call-site commands.
pub(super) fn evaluate_call_site_policy(
    selected_call_count: usize,
    fail_on_no_change: bool,
    require_calls: Option<usize>,
) -> super::types::CallSitePolicy {
    let mut violations = Vec::new();
    if fail_on_no_change && selected_call_count == 0 {
        violations.push("no selected call site changed".to_owned());
    }
    if let Some(required) = require_calls {
        if selected_call_count < required {
            violations.push(format!(
                "expected at least {required} changed call sites but found {selected_call_count}"
            ));
        }
    }
    super::types::CallSitePolicy {
        fail_on_no_change,
        require_calls,
        passed: violations.is_empty(),
        violations,
    }
}

/// One planned file for the callable rename family, mapped from the
/// per-command usecase plan types (which stay separate in the public API).
pub(super) struct CallableRenamePlanData {
    pub(super) dialect: Dialect,
    pub(super) definitions: Vec<rename_usecase::RenameFunctionOccurrence>,
    pub(super) calls: Vec<rename_usecase::RenameFunctionOccurrence>,
    pub(super) rewritten: String,
    pub(super) changed: bool,
}

pub(super) struct CallableRenameCommand<'a> {
    pub(super) files: &'a [std::path::PathBuf],
    pub(super) dialect: Option<crate::presentation::cli::DialectArg>,
    pub(super) from: &'a SymbolName,
    pub(super) to: &'a SymbolName,
    pub(super) write: bool,
    pub(super) fail_on_no_change: bool,
    pub(super) output: crate::presentation::cli::OutputFormat,
    pub(super) command: &'static str,
    pub(super) missing_definition_error: &'static str,
}

/// Shared plan→write→report→gate runner for rename-function,
/// rename-macrolet, and rename-local-function.
pub(super) fn run_callable_rename(
    command: CallableRenameCommand<'_>,
    plan: impl Fn(&str, Dialect) -> Result<CallableRenamePlanData>,
) -> Result<()> {
    use anyhow::Context;

    let mut pending = Vec::with_capacity(command.files.len());
    let mut definition_count = 0usize;

    for file in command.files {
        let (input, dialect) = crate::presentation::cli::shared::read_input_and_dialect(
            Some(file.clone()),
            command.dialect,
        )?;
        let plan_data = plan(&input.text, dialect).with_context(|| {
            format!("failed to plan {} for {}", command.command, file.display())
        })?;
        definition_count += plan_data.definitions.len();
        pending.push(super::types::PendingCallableRenameFile {
            path: file.clone(),
            dialect: plan_data.dialect,
            definitions: plan_data.definitions,
            calls: plan_data.calls,
            rewritten: plan_data.rewritten,
            changed: plan_data.changed,
        });
    }

    if definition_count == 0 {
        anyhow::bail!("{}", command.missing_definition_error);
    }

    let written_files = pending
        .iter()
        .filter(|file| command.write && file.changed)
        .map(|file| (file.path.clone(), file.rewritten.clone()))
        .collect::<Vec<_>>();
    if !written_files.is_empty() {
        crate::presentation::cli::shared::write_files_with_rollback(written_files)?;
    }

    let mut reports = Vec::with_capacity(pending.len());
    for file in pending {
        let written = command.write && file.changed;
        reports.push(super::types::CallableRenameFileReport {
            path: file.path,
            dialect: file.dialect,
            definitions: file.definitions,
            calls: file.calls,
            changed: file.changed,
            written,
            rewritten: file.rewritten,
        });
    }

    let changed = reports.iter().any(|report| report.changed);
    super::render::callable::print_callable_rename_report(
        &reports,
        command.from,
        command.to,
        command.write,
        command.output,
    )?;
    ensure_rename_changed(command.fail_on_no_change, changed, command.command)
}
