use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::workspace_report::usecase::types::{
    LoadedWorkspaceFile, WorkspaceFileMetrics, WorkspaceFileStatus, WorkspaceInventory,
    WorkspaceReportRequest, WorkspaceReportSourcePort,
};
use crate::workspace_report::usecase::workflow::{
    build_workspace_report, summarize_workspace_report,
};
use paredit_core_syntax::dialect::Dialect;

struct FakeSource {
    inventory: Option<WorkspaceInventory>,
    files: BTreeMap<PathBuf, (Dialect, Result<Vec<u8>, String>)>,
}

impl WorkspaceReportSourcePort for FakeSource {
    type Error = paredit_core_cli::CliError;

    fn discover(
        &mut self,
        _request: &WorkspaceReportRequest,
    ) -> Result<WorkspaceInventory, Self::Error> {
        Ok(self.inventory.take().expect("discovery called once"))
    }

    fn load(&self, path: &Path) -> LoadedWorkspaceFile {
        let (dialect, bytes) = self.files.get(path).expect("known file");
        LoadedWorkspaceFile {
            dialect: *dialect,
            bytes: bytes.clone(),
        }
    }
}

#[test]
fn summary_counts_files_by_status_and_dialect() {
    let parsed = WorkspaceFileStatus::Parsed;
    let parse_error = WorkspaceFileStatus::ParseError("broken".to_owned());

    let summary = summarize_workspace_report([
        WorkspaceFileMetrics {
            dialect: Dialect::CommonLisp,
            status: &parsed,
            byte_count: 10,
            top_level_form_count: 1,
            atom_count: 3,
            definition_count: 1,
            call_count: 2,
            max_complexity_score: 0,
        },
        WorkspaceFileMetrics {
            dialect: Dialect::EmacsLisp,
            status: &parse_error,
            byte_count: 5,
            top_level_form_count: 0,
            atom_count: 0,
            definition_count: 0,
            call_count: 0,
            max_complexity_score: 0,
        },
    ]);

    assert_eq!(summary.file_count, 2);
    assert_eq!(summary.parsed_count, 1);
    assert_eq!(summary.parse_error_count, 1);
    assert_eq!(summary.byte_count, 15);
    assert_eq!(summary.dialect_counts["common-lisp"], 1);
    assert_eq!(summary.dialect_counts["emacs-lisp"], 1);
    assert_eq!(summary.status_counts["parsed"], 1);
    assert_eq!(summary.status_counts["parse-error"], 1);
}

#[test]
fn report_preserves_discovery_order_and_aggregates_inventory() {
    let first = PathBuf::from("z.lisp");
    let second = PathBuf::from("a.el");
    let mut source = FakeSource {
        inventory: Some(WorkspaceInventory {
            files: vec![first.clone(), second.clone()],
            skipped_unknown_count: 1,
            skipped_hidden_count: 2,
            skipped_generated_count: 3,
            skipped_symlink_count: 4,
        }),
        files: BTreeMap::from([
            (
                first.clone(),
                (
                    Dialect::CommonLisp,
                    Ok(b"(defun hello () (print \"hello\"))".to_vec()),
                ),
            ),
            (
                second.clone(),
                (
                    Dialect::EmacsLisp,
                    Ok(b"(defun goodbye () (message \"goodbye\"))".to_vec()),
                ),
            ),
        ]),
    };

    let plan = build_workspace_report(
        &mut source,
        WorkspaceReportRequest {
            roots: vec![PathBuf::from(".")],
            include_unknown: false,
            include_hidden: false,
            include_generated: false,
            max_depth: None,
        },
    )
    .expect("workspace report");

    assert_eq!(
        plan.reports
            .iter()
            .map(|report| report.path.as_path())
            .collect::<Vec<_>>(),
        vec![first.as_path(), second.as_path()]
    );
    assert_eq!(plan.summary.file_count, 2);
    assert_eq!(plan.summary.definition_count, 2);
    assert_eq!(plan.skipped_unknown_count, 1);
    assert_eq!(plan.skipped_hidden_count, 2);
    assert_eq!(plan.skipped_generated_count, 3);
    assert_eq!(plan.skipped_symlink_count, 4);
}

#[test]
fn report_converts_load_and_utf8_failures_to_parse_errors() {
    let unreadable = PathBuf::from("unreadable.lisp");
    let invalid_utf8 = PathBuf::from("invalid.el");
    let mut source = FakeSource {
        inventory: Some(WorkspaceInventory {
            files: vec![unreadable.clone(), invalid_utf8.clone()],
            skipped_unknown_count: 0,
            skipped_hidden_count: 0,
            skipped_generated_count: 0,
            skipped_symlink_count: 0,
        }),
        files: BTreeMap::from([
            (
                unreadable,
                (Dialect::CommonLisp, Err("permission denied".to_owned())),
            ),
            (invalid_utf8, (Dialect::EmacsLisp, Ok(vec![0xff, 0xfe]))),
        ]),
    };

    let plan = build_workspace_report(
        &mut source,
        WorkspaceReportRequest {
            roots: Vec::new(),
            include_unknown: false,
            include_hidden: false,
            include_generated: false,
            max_depth: None,
        },
    )
    .expect("workspace report");

    assert_eq!(plan.summary.parse_error_count, 2);
    assert_eq!(plan.reports[0].byte_count, 0);
    assert_eq!(plan.reports[1].byte_count, 2);
    assert!(matches!(
        &plan.reports[0].status,
        WorkspaceFileStatus::ParseError(error) if error == "permission denied"
    ));
    assert!(matches!(
        &plan.reports[1].status,
        WorkspaceFileStatus::ParseError(error) if error.contains("invalid utf-8")
    ));
}
