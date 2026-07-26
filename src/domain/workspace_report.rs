use std::collections::BTreeMap;

use crate::domain::dialect::Dialect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceFileStatus {
    Parsed,
    ParseError(String),
}

impl WorkspaceFileStatus {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Parsed => "parsed",
            Self::ParseError(_) => "parse-error",
        }
    }

    #[must_use]
    pub fn is_parsed(&self) -> bool {
        matches!(self, Self::Parsed)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceFileMetrics<'a> {
    pub dialect: Dialect,
    pub status: &'a WorkspaceFileStatus,
    pub byte_count: usize,
    pub top_level_form_count: usize,
    pub atom_count: usize,
    pub definition_count: usize,
    pub call_count: usize,
    /// Highest per-definition complexity score in this file, or `0` when the
    /// file has no definition-like top-level forms. See
    /// `crate::domain::complexity_report` for the scoring formula.
    pub max_complexity_score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceReportSummary {
    pub file_count: usize,
    pub parsed_count: usize,
    pub parse_error_count: usize,
    pub byte_count: usize,
    pub top_level_form_count: usize,
    pub atom_count: usize,
    pub definition_count: usize,
    pub call_count: usize,
    /// Highest per-definition complexity score across every file.
    pub max_complexity_score: usize,
    pub dialect_counts: BTreeMap<&'static str, usize>,
    pub status_counts: BTreeMap<&'static str, usize>,
}

pub fn summarize_workspace_report<'a>(
    files: impl IntoIterator<Item = WorkspaceFileMetrics<'a>>,
) -> WorkspaceReportSummary {
    let mut summary = WorkspaceReportSummary {
        file_count: 0,
        parsed_count: 0,
        parse_error_count: 0,
        byte_count: 0,
        top_level_form_count: 0,
        atom_count: 0,
        definition_count: 0,
        call_count: 0,
        max_complexity_score: 0,
        dialect_counts: BTreeMap::new(),
        status_counts: BTreeMap::new(),
    };

    for file in files {
        summary.file_count += 1;
        if file.status.is_parsed() {
            summary.parsed_count += 1;
        } else {
            summary.parse_error_count += 1;
        }
        summary.byte_count += file.byte_count;
        summary.top_level_form_count += file.top_level_form_count;
        summary.atom_count += file.atom_count;
        summary.definition_count += file.definition_count;
        summary.call_count += file.call_count;
        summary.max_complexity_score = summary.max_complexity_score.max(file.max_complexity_score);
        *summary
            .dialect_counts
            .entry(file.dialect.label())
            .or_default() += 1;
        *summary
            .status_counts
            .entry(file.status.label())
            .or_default() += 1;
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_files_by_status_and_dialect() {
        let parsed = WorkspaceFileStatus::Parsed;
        let parse_error = WorkspaceFileStatus::ParseError("broken".to_owned());
        let summary = summarize_workspace_report([
            WorkspaceFileMetrics {
                dialect: Dialect::CommonLisp,
                status: &parsed,
                byte_count: 10,
                top_level_form_count: 2,
                atom_count: 3,
                definition_count: 1,
                call_count: 2,
                max_complexity_score: 7,
            },
            WorkspaceFileMetrics {
                dialect: Dialect::EmacsLisp,
                status: &parse_error,
                byte_count: 5,
                top_level_form_count: 1,
                atom_count: 2,
                definition_count: 0,
                call_count: 0,
                max_complexity_score: 0,
            },
        ]);

        assert_eq!(summary.file_count, 2);
        assert_eq!(summary.parsed_count, 1);
        assert_eq!(summary.parse_error_count, 1);
        assert_eq!(summary.byte_count, 15);
        assert_eq!(summary.dialect_counts.get("common-lisp"), Some(&1));
        assert_eq!(summary.status_counts.get("parse-error"), Some(&1));
        assert_eq!(summary.max_complexity_score, 7);
    }

    #[test]
    fn max_complexity_score_tracks_the_single_highest_file() {
        let parsed = WorkspaceFileStatus::Parsed;
        let summary = summarize_workspace_report([
            WorkspaceFileMetrics {
                dialect: Dialect::CommonLisp,
                status: &parsed,
                byte_count: 1,
                top_level_form_count: 1,
                atom_count: 1,
                definition_count: 1,
                call_count: 0,
                max_complexity_score: 12,
            },
            WorkspaceFileMetrics {
                dialect: Dialect::CommonLisp,
                status: &parsed,
                byte_count: 1,
                top_level_form_count: 1,
                atom_count: 1,
                definition_count: 1,
                call_count: 0,
                max_complexity_score: 4,
            },
        ]);

        assert_eq!(summary.max_complexity_score, 12);
    }
}
