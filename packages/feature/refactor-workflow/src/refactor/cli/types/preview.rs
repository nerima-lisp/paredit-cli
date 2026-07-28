use super::super::args::RefactorPreviewMode;
use super::plan::WorkspaceRefactorPlanDiscovery;
use crate::refactor::usecase::execute::{
    RefactorWriteCandidate, RefactorWritePlan, build_refactor_write_plan,
};
use crate::refactor::usecase::preview::{
    RefactorPreviewEdit, RefactorPreviewPolicy, RefactorPreviewSummary,
};
use paredit_core_edit::refactor_preview::{
    PreviewPolicy, PreviewWriteParse, PreviewWriteRequest, RefactorPreviewDecisionStatus,
    decide_refactor_preview,
};
use paredit_core_syntax::dialect::Dialect;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RefactorPreview {
    pub workspace: Option<WorkspaceRefactorPlanDiscovery>,
    pub mode: RefactorPreviewMode,
    pub from: String,
    pub to: String,
    pub write_requested: bool,
    pub files: Vec<RefactorPreviewFile>,
    pub summary: RefactorPreviewSummary,
    pub policy: RefactorPreviewPolicy,
}

impl RefactorPreview {
    #[must_use]
    pub fn write_plan(&self) -> RefactorWritePlan {
        let candidates = self
            .files
            .iter()
            .map(|file| RefactorWriteCandidate {
                changed: file.changed,
                output_parse_ok: file.output_parse_ok,
            })
            .collect::<Vec<_>>();

        build_refactor_write_plan(self.write_requested, &candidates)
    }

    #[must_use]
    pub fn writable_paths_for_write_plan(&self, write_plan: &RefactorWritePlan) -> Vec<String> {
        write_plan
            .writable_indexes()
            .iter()
            .filter_map(|index| self.files.get(*index))
            .map(|file| file.path.display().to_string())
            .collect()
    }

    #[must_use]
    pub fn refused_paths_for_write_plan(&self, write_plan: &RefactorWritePlan) -> Vec<String> {
        if write_plan.refusal().is_none() {
            return Vec::new();
        }

        self.files
            .iter()
            .filter(|file| file.changed && !file.output_parse_ok)
            .map(|file| file.path.display().to_string())
            .collect()
    }

    #[must_use]
    pub fn decision_for_write_plan(
        &self,
        write_plan: &RefactorWritePlan,
    ) -> RefactorPreviewDecision {
        RefactorPreviewDecision {
            status: decide_refactor_preview(
                if self.write_requested {
                    PreviewWriteRequest::Requested
                } else {
                    PreviewWriteRequest::NotRequested
                },
                if self.policy.passed() {
                    PreviewPolicy::Passed
                } else {
                    PreviewPolicy::Failed
                },
                if write_plan.refusal().is_some() {
                    PreviewWriteParse::Refused
                } else {
                    PreviewWriteParse::Accepted
                },
            ),
        }
    }
}

#[derive(Debug)]
pub struct RefactorPreviewDecision {
    pub status: RefactorPreviewDecisionStatus,
}

impl RefactorPreviewDecision {
    #[must_use]
    pub const fn write_parse_refused(&self) -> bool {
        matches!(
            self.status,
            RefactorPreviewDecisionStatus::RefusedUnparsableOutput
        )
    }

    #[must_use]
    pub const fn apply_preview(&self) -> bool {
        matches!(self.status, RefactorPreviewDecisionStatus::WriteApplied)
    }

    #[must_use]
    pub const fn steps(&self) -> [RefactorPreviewDecisionStep; 3] {
        [
            RefactorPreviewDecisionStep {
                name: "preview-policy",
                status: match self.status {
                    RefactorPreviewDecisionStatus::BlockedByPolicy => {
                        RefactorPreviewDecisionStepStatus::Failed
                    }
                    _ => RefactorPreviewDecisionStepStatus::Passed,
                },
            },
            RefactorPreviewDecisionStep {
                name: "write-output-parse",
                status: match self.status {
                    RefactorPreviewDecisionStatus::BlockedByPolicy => {
                        RefactorPreviewDecisionStepStatus::Skipped
                    }
                    RefactorPreviewDecisionStatus::RefusedUnparsableOutput => {
                        RefactorPreviewDecisionStepStatus::Failed
                    }
                    _ => RefactorPreviewDecisionStepStatus::Passed,
                },
            },
            RefactorPreviewDecisionStep {
                name: "apply-preview",
                status: match self.status {
                    RefactorPreviewDecisionStatus::WriteApplied => {
                        RefactorPreviewDecisionStepStatus::Passed
                    }
                    RefactorPreviewDecisionStatus::DryRunReady => {
                        RefactorPreviewDecisionStepStatus::Scheduled
                    }
                    RefactorPreviewDecisionStatus::BlockedByPolicy
                    | RefactorPreviewDecisionStatus::RefusedUnparsableOutput => {
                        RefactorPreviewDecisionStepStatus::Skipped
                    }
                },
            },
        ]
    }
}

#[derive(Debug)]
pub struct RefactorPreviewDecisionStep {
    pub name: &'static str,
    pub status: RefactorPreviewDecisionStepStatus,
}

#[derive(Debug, Clone, Copy)]
pub enum RefactorPreviewDecisionStepStatus {
    Passed,
    Failed,
    Skipped,
    Scheduled,
}

impl RefactorPreviewDecisionStepStatus {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug)]
pub struct RefactorPreviewFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub changed: bool,
    pub written: bool,
    pub edit_count: usize,
    pub edits: Vec<RefactorPreviewEdit>,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub output_parse_ok: bool,
    pub input_hash: String,
    pub output_hash: String,
    pub preview: String,
    pub rewritten: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::usecase::preview::RefactorPreviewPolicyOptions as DomainRefactorPreviewPolicyOptions;
    use crate::refactor::usecase::preview::evaluate_refactor_preview_policy;

    fn preview_file(path: &str, changed: bool, output_parse_ok: bool) -> RefactorPreviewFile {
        RefactorPreviewFile {
            path: PathBuf::from(path),
            dialect: Dialect::CommonLisp,
            changed,
            written: false,
            edit_count: usize::from(changed),
            edits: Vec::new(),
            input_bytes: 0,
            output_bytes: 0,
            output_parse_ok,
            input_hash: String::new(),
            output_hash: String::new(),
            preview: String::new(),
            rewritten: String::new(),
        }
    }

    fn preview(files: Vec<RefactorPreviewFile>, write_requested: bool) -> RefactorPreview {
        RefactorPreview {
            workspace: None,
            mode: RefactorPreviewMode::Symbol,
            from: "old-name".to_string(),
            to: "new-name".to_string(),
            write_requested,
            files,
            summary: RefactorPreviewSummary::new(Vec::new(), 0, 0, 0, 0, 0),
            policy: evaluate_refactor_preview_policy(
                DomainRefactorPreviewPolicyOptions::new(false, false, false, None, None, None)
                    .expect("valid policy options"),
                &RefactorPreviewSummary::new(Vec::new(), 0, 0, 0, 0, 0),
            ),
        }
    }

    #[test]
    fn refused_paths_for_write_plan_lists_changed_unparsable_files_only_when_refused() {
        let preview = preview(
            vec![
                preview_file("ok.lisp", true, true),
                preview_file("broken.lisp", true, false),
                preview_file("unchanged-broken.lisp", false, false),
            ],
            true,
        );

        let write_plan = preview.write_plan();

        assert_eq!(
            preview.refused_paths_for_write_plan(&write_plan),
            vec!["broken.lisp".to_string()]
        );
    }

    #[test]
    fn refused_paths_for_write_plan_is_empty_without_write_refusal() {
        let preview = preview(
            vec![preview_file("dry-run-broken.lisp", true, false)],
            false,
        );

        let write_plan = preview.write_plan();

        assert!(preview.refused_paths_for_write_plan(&write_plan).is_empty());
    }

    #[test]
    fn decision_steps_schedule_apply_preview_for_dry_run_ready() {
        let preview = preview(vec![preview_file("core.lisp", true, true)], false);
        let write_plan = preview.write_plan();
        let decision = preview.decision_for_write_plan(&write_plan);
        let steps = decision.steps();

        assert_eq!(steps[0].name, "preview-policy");
        assert_eq!(steps[0].status.label(), "passed");
        assert_eq!(steps[1].name, "write-output-parse");
        assert_eq!(steps[1].status.label(), "passed");
        assert_eq!(steps[2].name, "apply-preview");
        assert_eq!(steps[2].status.label(), "scheduled");
    }

    #[test]
    fn decision_steps_pass_apply_preview_after_write() {
        let preview = preview(vec![preview_file("core.lisp", true, true)], true);
        let write_plan = preview.write_plan();
        let decision = preview.decision_for_write_plan(&write_plan);
        let steps = decision.steps();

        assert_eq!(steps[2].name, "apply-preview");
        assert_eq!(steps[2].status.label(), "passed");
    }

    #[test]
    fn decision_steps_skip_apply_preview_when_policy_blocks() {
        let mut preview = preview(vec![preview_file("core.lisp", true, true)], true);
        preview.policy = evaluate_refactor_preview_policy(
            DomainRefactorPreviewPolicyOptions::new(true, false, false, None, None, None)
                .expect("valid policy options"),
            &preview.summary,
        );
        let write_plan = preview.write_plan();

        let steps = preview.decision_for_write_plan(&write_plan).steps();

        assert_eq!(steps[0].status.label(), "failed");
        assert_eq!(steps[1].status.label(), "skipped");
        assert_eq!(steps[2].status.label(), "skipped");
    }
}
