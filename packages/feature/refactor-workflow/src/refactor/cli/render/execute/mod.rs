pub mod json;
pub mod text;

use super::super::types::execute::WorkspaceRefactorExecute;
use anyhow::Result;
use paredit_core_cli::args::OutputFormat;

pub fn print_workspace_refactor_execute(
    execution: &WorkspaceRefactorExecute,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_workspace_refactor_execute_text(execution),
        OutputFormat::Json => json::print_workspace_refactor_execute_json(execution),
    }
}
