use super::super::super::*;
use super::super::args::RefactorPreviewMode;
use super::plan::WorkspaceRefactorPlanDiscovery;
use crate::application::refactor::execute::{
    RefactorWriteCandidate, RefactorWritePlan, build_refactor_write_plan,
};
use crate::domain::refactor_preview::{RefactorPreviewDecisionStatus, decide_refactor_preview};

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorPreview {
    pub(in crate::presentation::cli) workspace: Option<WorkspaceRefactorPlanDiscovery>,
    pub(in crate::presentation::cli) mode: RefactorPreviewMode,
    pub(in crate::presentation::cli) from: String,
    pub(in crate::presentation::cli) to: String,
    pub(in crate::presentation::cli) write_requested: bool,
    pub(in crate::presentation::cli) files: Vec<RefactorPreviewFile>,
    pub(in crate::presentation::cli) summary: RefactorPreviewSummary,
    pub(in crate::presentation::cli) policy: RefactorPreviewPolicy,
}

impl RefactorPreview {
    pub(in crate::presentation::cli) fn write_plan(&self) -> RefactorWritePlan {
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

    pub(in crate::presentation::cli) fn writable_paths_for_write_plan(
        &self,
        write_plan: &RefactorWritePlan,
    ) -> Vec<String> {
        write_plan
            .writable_indexes()
            .iter()
            .filter_map(|index| self.files.get(*index))
            .map(|file| file.path.display().to_string())
            .collect()
    }

    pub(in crate::presentation::cli) fn refused_paths_for_write_plan(
        &self,
        write_plan: &RefactorWritePlan,
    ) -> Vec<String> {
        if write_plan.refusal().is_none() {
            return Vec::new();
        }

        self.files
            .iter()
            .filter(|file| file.changed && !file.output_parse_ok)
            .map(|file| file.path.display().to_string())
            .collect()
    }

    pub(in crate::presentation::cli) fn decision_for_write_plan(
        &self,
        write_plan: &RefactorWritePlan,
    ) -> RefactorPreviewDecision {
        RefactorPreviewDecision {
            status: decide_refactor_preview(
                self.write_requested,
                self.policy.passed(),
                write_plan.refusal().is_some(),
            ),
        }
    }
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorPreviewDecision {
    pub(in crate::presentation::cli) status: RefactorPreviewDecisionStatus,
}

impl RefactorPreviewDecision {
    pub(in crate::presentation::cli) const fn write_parse_refused(&self) -> bool {
        matches!(
            self.status,
            RefactorPreviewDecisionStatus::RefusedUnparsableOutput
        )
    }

    pub(in crate::presentation::cli) const fn apply_preview(&self) -> bool {
        matches!(self.status, RefactorPreviewDecisionStatus::WriteApplied)
    }

    pub(in crate::presentation::cli) const fn steps(&self) -> [RefactorPreviewDecisionStep; 3] {
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
pub(in crate::presentation::cli) struct RefactorPreviewDecisionStep {
    pub(in crate::presentation::cli) name: &'static str,
    pub(in crate::presentation::cli) status: RefactorPreviewDecisionStepStatus,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::presentation::cli) enum RefactorPreviewDecisionStepStatus {
    Passed,
    Failed,
    Skipped,
    Scheduled,
}

impl RefactorPreviewDecisionStepStatus {
    pub(in crate::presentation::cli) const fn label(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorPreviewFile {
    pub(in crate::presentation::cli) path: PathBuf,
    pub(in crate::presentation::cli) dialect: Dialect,
    pub(in crate::presentation::cli) changed: bool,
    pub(in crate::presentation::cli) written: bool,
    pub(in crate::presentation::cli) edit_count: usize,
    pub(in crate::presentation::cli) edits: Vec<RefactorPreviewEdit>,
    pub(in crate::presentation::cli) input_bytes: usize,
    pub(in crate::presentation::cli) output_bytes: usize,
    pub(in crate::presentation::cli) output_parse_ok: bool,
    pub(in crate::presentation::cli) input_hash: String,
    pub(in crate::presentation::cli) output_hash: String,
    pub(in crate::presentation::cli) preview: String,
    pub(in crate::presentation::cli) rewritten: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
