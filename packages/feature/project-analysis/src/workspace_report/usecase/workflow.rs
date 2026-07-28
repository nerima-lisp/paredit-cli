use anyhow::Result;

use crate::error::WorkspaceAnalysisError;

use crate::call_report::usecase::build_call_report;
use crate::complexity_report::usecase::build_complexity_report;
pub use crate::workspace_report::domain::summarize_workspace_report;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_feature_remove_unused::definition_report::usecase::collect_definition_forms;

use super::types::{
    WorkspaceFileMetrics, WorkspaceFileReport, WorkspaceFileStatus, WorkspaceReportPlan,
    WorkspaceReportRequest, WorkspaceReportSourcePort,
};

pub fn build_workspace_report(
    source: &mut impl WorkspaceReportSourcePort,
    request: WorkspaceReportRequest,
) -> Result<WorkspaceReportPlan> {
    let inventory = source.discover(&request)?;
    let mut reports = Vec::with_capacity(inventory.files.len());

    for file in inventory.files {
        let loaded = source.load(&file);
        let bytes = match loaded.bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                reports.push(parse_error_report(file, loaded.dialect, 0, error));
                continue;
            }
        };
        let byte_count = bytes.len();
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                reports.push(parse_error_report(file, loaded.dialect, byte_count, error));
                continue;
            }
        };

        match SyntaxTree::parse_with_dialect(&text, loaded.dialect) {
            Ok(tree) => {
                let (package, definitions) = collect_definition_forms(&tree, loaded.dialect)
                    .map_err(|source| WorkspaceAnalysisError::Definitions {
                        path: file.display().to_string(),
                        source: Box::new(source.into()),
                    })?;
                let calls =
                    build_call_report(&tree, loaded.dialect, None, false).map_err(|source| {
                        WorkspaceAnalysisError::Calls {
                            path: file.display().to_string(),
                            source: Box::new(source),
                        }
                    })?;
                // `definitions` is sorted by descending complexity score, so
                // the first entry (if any) is the file's highest score.
                let max_complexity_score =
                    build_complexity_report(file.clone(), loaded.dialect, &tree)
                        .map_err(|source| WorkspaceAnalysisError::Complexity {
                            path: file.display().to_string(),
                            source: Box::new(source),
                        })?
                        .definitions
                        .first()
                        .map_or(0, |definition| definition.complexity_score);
                reports.push(WorkspaceFileReport {
                    path: file,
                    dialect: loaded.dialect,
                    status: WorkspaceFileStatus::Parsed,
                    byte_count,
                    top_level_form_count: tree.root_children().len(),
                    atom_count: tree.atom_occurrence_count(),
                    definition_count: definitions.len(),
                    call_count: calls.len(),
                    max_complexity_score,
                    package,
                });
            }
            Err(error) => {
                reports.push(parse_error_report(file, loaded.dialect, byte_count, error));
            }
        }
    }

    let summary = summarize_workspace_report(reports.iter().map(|report| WorkspaceFileMetrics {
        dialect: report.dialect,
        status: &report.status,
        byte_count: report.byte_count,
        top_level_form_count: report.top_level_form_count,
        atom_count: report.atom_count,
        definition_count: report.definition_count,
        call_count: report.call_count,
        max_complexity_score: report.max_complexity_score,
    }));

    Ok(WorkspaceReportPlan {
        roots: request.roots,
        reports,
        summary,
        skipped_unknown_count: inventory.skipped_unknown_count,
        skipped_hidden_count: inventory.skipped_hidden_count,
        skipped_generated_count: inventory.skipped_generated_count,
        skipped_symlink_count: inventory.skipped_symlink_count,
    })
}

fn parse_error_report(
    path: std::path::PathBuf,
    dialect: paredit_core_syntax::dialect::Dialect,
    byte_count: usize,
    error: impl std::fmt::Display,
) -> WorkspaceFileReport {
    WorkspaceFileReport {
        path,
        dialect,
        status: WorkspaceFileStatus::ParseError(error.to_string()),
        byte_count,
        top_level_form_count: 0,
        atom_count: 0,
        definition_count: 0,
        call_count: 0,
        max_complexity_score: 0,
        package: None,
    }
}
