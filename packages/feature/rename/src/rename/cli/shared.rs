use crate::error::{RenameError, RenamePlanError};
use paredit_core_cli::{CliResult, CommandResult};

use crate::rename::usecase::{self as rename_usecase, RenameTarget};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Path, SymbolName, SyntaxTree};

pub fn rename_target(path: Option<Path>, at: Option<usize>) -> CliResult<RenameTarget> {
    match (path, at) {
        (Some(path), None) => Ok(RenameTarget::Path(path)),
        (None, Some(offset)) => Ok(RenameTarget::Offset(offset)),
        // The two argument errors core already names, rather than two
        // sentences that happen to match theirs.
        (None, None) => Err(paredit_core_cli::ArgumentError::TargetRequired.into()),
        (Some(_), Some(_)) => Err(paredit_core_cli::ArgumentError::TargetAmbiguous.into()),
    }
}

pub fn collect_callable_definition_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> CliResult<Vec<rename_usecase::RenameFunctionOccurrence>> {
    Ok(rename_usecase::collect_callable_definition_renames(
        tree, dialect, from, to,
    )?)
}

pub fn collect_function_call_head_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> CliResult<Vec<rename_usecase::RenameFunctionOccurrence>> {
    Ok(rename_usecase::collect_function_call_head_renames(
        tree, dialect, from, to,
    )?)
}

pub fn ensure_rename_changed(
    fail_on_no_change: bool,
    changed: bool,
    command: &str,
) -> CommandResult {
    if fail_on_no_change && !changed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "{command} policy failed: no occurrence changed"
        )));
    }
    Ok(())
}

/// Evaluates the shared --fail-on-no-change / --require-calls policy used by
/// the wrap/replace/unwrap call-site commands.
#[must_use]
pub fn evaluate_call_site_policy(
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
// Public since the extraction: crate-internal visibility cannot cross a
// crate boundary, so this lint applies for the first time.
#[derive(Debug)]
pub struct CallableRenamePlanData {
    pub dialect: Dialect,
    pub definitions: Vec<rename_usecase::RenameFunctionOccurrence>,
    pub calls: Vec<rename_usecase::RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

// Public since the extraction: crate-internal visibility cannot cross a
// crate boundary, so this lint applies for the first time.
#[derive(Debug)]
pub struct CallableRenameCommand<'a> {
    pub files: &'a [std::path::PathBuf],
    pub dialect: Option<paredit_core_cli::args::DialectArg>,
    pub from: &'a SymbolName,
    pub to: &'a SymbolName,
    pub write: bool,
    pub fail_on_no_change: bool,
    pub output: paredit_core_cli::args::OutputFormat,
    pub command: &'static str,
    pub missing_definition_error: &'static str,
}

/// Shared plan→write→report→gate runner for rename-function,
/// rename-macrolet, and rename-local-function.
pub fn run_callable_rename(
    command: CallableRenameCommand<'_>,
    plan: impl Fn(&str, Dialect) -> Result<CallableRenamePlanData, RenameError>,
) -> CommandResult {
    let mut pending = Vec::with_capacity(command.files.len());
    let mut definition_count = 0usize;

    for file in command.files {
        let (input, dialect) =
            paredit_core_cli::shared::read_input_and_dialect(Some(file.clone()), command.dialect)?;
        let plan_data = plan(&input.text, dialect).map_err(|source| RenamePlanError {
            operation: (command.command).to_string(),
            path: file.display().to_string(),
            source,
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
        // Every caller's `missing_definition_error` says "no definition of
        // this kind matched", which is a selection failure, not a defect.
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::SelectionNoMatch,
            command.missing_definition_error,
        )
        .into());
    }

    let written_files = pending
        .iter()
        .filter(|file| command.write && file.changed)
        .map(|file| (file.path.clone(), file.rewritten.clone()))
        .collect::<Vec<_>>();
    if !written_files.is_empty() {
        paredit_core_cli::shared::write_files_with_rollback(written_files)?;
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
