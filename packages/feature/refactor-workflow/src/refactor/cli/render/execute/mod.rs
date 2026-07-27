mod json;
mod text;

use super::super::super::*;
use super::super::types::execute::WorkspaceRefactorExecute;

pub(in crate::presentation::cli) fn print_workspace_refactor_execute(
    execution: &WorkspaceRefactorExecute,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_workspace_refactor_execute_text(execution),
        OutputFormat::Json => json::print_workspace_refactor_execute_json(execution),
    }
}
