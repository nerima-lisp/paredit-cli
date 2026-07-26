use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
pub struct RefactorPreviewSummary {
    changed_files: Vec<String>,
    unchanged_file_count: usize,
    written_file_count: usize,
    definition_count: usize,
    target_occurrence_count: usize,
    edit_count: usize,
    parse_status: RefactorPreviewParseStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorPreviewParseStatus {
    AllOutputsParse,
    ParseErrors(NonZeroUsize),
}

impl RefactorPreviewSummary {
    #[must_use]
    pub fn new(
        changed_files: Vec<String>,
        unchanged_file_count: usize,
        definition_count: usize,
        target_occurrence_count: usize,
        edit_count: usize,
        parse_error_count: usize,
    ) -> Self {
        Self {
            changed_files,
            unchanged_file_count,
            written_file_count: 0,
            definition_count,
            target_occurrence_count,
            edit_count,
            parse_status: if parse_error_count == 0 {
                RefactorPreviewParseStatus::AllOutputsParse
            } else {
                RefactorPreviewParseStatus::ParseErrors(
                    NonZeroUsize::new(parse_error_count)
                        .expect("non-zero parse error count was checked"),
                )
            },
        }
    }

    #[must_use]
    pub fn file_count(&self) -> usize {
        self.changed_file_count() + self.unchanged_file_count
    }

    #[must_use]
    pub fn changed_file_count(&self) -> usize {
        self.changed_files.len()
    }

    #[must_use]
    pub fn changed_files(&self) -> &[String] {
        &self.changed_files
    }

    #[must_use]
    pub const fn unchanged_file_count(&self) -> usize {
        self.unchanged_file_count
    }

    #[must_use]
    pub const fn written_file_count(&self) -> usize {
        self.written_file_count
    }

    pub fn set_written_file_count(&mut self, written_file_count: usize) -> Result<(), String> {
        if written_file_count > self.changed_file_count() {
            return Err(format!(
                "written file count ({written_file_count}) exceeds changed file count ({})",
                self.changed_file_count()
            ));
        }
        self.written_file_count = written_file_count;
        Ok(())
    }

    #[must_use]
    pub const fn definition_count(&self) -> usize {
        self.definition_count
    }

    #[must_use]
    pub const fn target_occurrence_count(&self) -> usize {
        self.target_occurrence_count
    }

    #[must_use]
    pub const fn edit_count(&self) -> usize {
        self.edit_count
    }

    #[must_use]
    pub const fn parse_error_count(&self) -> usize {
        match self.parse_status {
            RefactorPreviewParseStatus::AllOutputsParse => 0,
            RefactorPreviewParseStatus::ParseErrors(count) => count.get(),
        }
    }

    #[must_use]
    pub const fn all_outputs_parse(&self) -> bool {
        matches!(
            self.parse_status,
            RefactorPreviewParseStatus::AllOutputsParse
        )
    }
}

#[derive(Debug, Clone)]
pub struct RefactorPreviewPolicy {
    options: RefactorPreviewPolicyOptions,
    violations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorPreviewPolicyStatus {
    Passed,
    Failed,
}

impl RefactorPreviewPolicy {
    #[must_use]
    pub const fn fail_on_no_change(&self) -> bool {
        self.options.fail_on_no_change()
    }

    #[must_use]
    pub const fn fail_on_parse_error(&self) -> bool {
        self.options.fail_on_parse_error()
    }

    #[must_use]
    pub const fn fail_on_target_conflict(&self) -> bool {
        self.options.fail_on_target_conflict()
    }

    #[must_use]
    pub const fn require_changed_files(&self) -> Option<usize> {
        self.options.require_changed_files()
    }

    #[must_use]
    pub const fn require_definitions(&self) -> Option<usize> {
        self.options.require_definitions()
    }

    #[must_use]
    pub const fn require_edits(&self) -> Option<usize> {
        self.options.require_edits()
    }

    #[must_use]
    pub fn status(&self) -> RefactorPreviewPolicyStatus {
        if self.violations.is_empty() {
            RefactorPreviewPolicyStatus::Passed
        } else {
            RefactorPreviewPolicyStatus::Failed
        }
    }

    #[must_use]
    pub fn passed(&self) -> bool {
        self.status() == RefactorPreviewPolicyStatus::Passed
    }

    #[must_use]
    pub fn violations(&self) -> &[String] {
        &self.violations
    }

    #[must_use]
    pub fn summary(&self) -> RefactorPreviewPolicySummary {
        let violation_count = self.violations.len();

        RefactorPreviewPolicySummary {
            violation_count,
            write_blocked: violation_count > 0,
            next_action: if violation_count == 0 {
                "review-preview-or-rerun-with-write"
            } else {
                "review-policy-violations"
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorPreviewPolicySummary {
    pub violation_count: usize,
    pub write_blocked: bool,
    pub next_action: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorPreviewPolicyOptions {
    fail_on_no_change: bool,
    fail_on_parse_error: bool,
    fail_on_target_conflict: bool,
    require_changed_files: Option<usize>,
    require_definitions: Option<usize>,
    require_edits: Option<usize>,
}

impl RefactorPreviewPolicyOptions {
    pub fn new(
        fail_on_no_change: bool,
        fail_on_parse_error: bool,
        fail_on_target_conflict: bool,
        require_changed_files: Option<usize>,
        require_definitions: Option<usize>,
        require_edits: Option<usize>,
    ) -> Result<Self, String> {
        for (name, value) in [
            ("--require-changed-files", require_changed_files),
            ("--require-definitions", require_definitions),
            ("--require-edits", require_edits),
        ] {
            if value == Some(0) {
                return Err(format!("{name} must be greater than zero when specified"));
            }
        }

        Ok(Self {
            fail_on_no_change,
            fail_on_parse_error,
            fail_on_target_conflict,
            require_changed_files,
            require_definitions,
            require_edits,
        })
    }

    #[must_use]
    pub const fn fail_on_no_change(self) -> bool {
        self.fail_on_no_change
    }

    #[must_use]
    pub const fn fail_on_parse_error(self) -> bool {
        self.fail_on_parse_error
    }

    #[must_use]
    pub const fn fail_on_target_conflict(self) -> bool {
        self.fail_on_target_conflict
    }

    #[must_use]
    pub const fn require_changed_files(self) -> Option<usize> {
        self.require_changed_files
    }

    #[must_use]
    pub const fn require_definitions(self) -> Option<usize> {
        self.require_definitions
    }

    #[must_use]
    pub const fn require_edits(self) -> Option<usize> {
        self.require_edits
    }
}

#[must_use]
pub fn evaluate_refactor_preview_policy(
    options: RefactorPreviewPolicyOptions,
    summary: &RefactorPreviewSummary,
) -> RefactorPreviewPolicy {
    let mut violations = Vec::new();

    if options.fail_on_no_change() && summary.changed_file_count() == 0 {
        violations.push("--fail-on-no-change expected at least one changed file".to_owned());
    }

    if options.fail_on_parse_error() && !summary.all_outputs_parse() {
        violations.push(format!(
            "--fail-on-parse-error found {} unparsable output file(s)",
            summary.parse_error_count()
        ));
    }

    if options.fail_on_target_conflict() && summary.target_occurrence_count() > 0 {
        violations.push(format!(
            "--fail-on-target-conflict found {} existing replacement symbol occurrence(s)",
            summary.target_occurrence_count()
        ));
    }

    if let Some(required) = options.require_changed_files() {
        if summary.changed_file_count() < required {
            violations.push(format!(
                "--require-changed-files expected at least {required}, found {}",
                summary.changed_file_count()
            ));
        }
    }

    if let Some(required) = options.require_definitions() {
        if summary.definition_count() != required {
            violations.push(format!(
                "--require-definitions expected exactly {required}, found {}",
                summary.definition_count()
            ));
        }
    }

    if let Some(required) = options.require_edits() {
        if summary.edit_count() < required {
            violations.push(format!(
                "--require-edits expected at least {required}, found {}",
                summary.edit_count()
            ));
        }
    }

    RefactorPreviewPolicy {
        options,
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RefactorPreviewPolicyOptions, RefactorPreviewPolicyStatus, RefactorPreviewSummary,
        evaluate_refactor_preview_policy,
    };

    #[test]
    fn policy_options_reject_zero_thresholds() {
        let result = RefactorPreviewPolicyOptions::new(true, true, true, Some(0), None, None);

        assert_eq!(
            result.unwrap_err(),
            "--require-changed-files must be greater than zero when specified"
        );
    }

    #[test]
    fn summary_derives_counts_and_parse_state() {
        let summary = RefactorPreviewSummary::new(vec!["changed.lisp".to_owned()], 2, 3, 4, 5, 1);

        assert_eq!(summary.file_count(), 3);
        assert_eq!(summary.changed_file_count(), 1);
        assert_eq!(summary.parse_error_count(), 1);
        assert!(!summary.all_outputs_parse());
    }

    #[test]
    fn summary_rejects_more_written_files_than_changed_files() {
        let mut summary =
            RefactorPreviewSummary::new(vec!["changed.lisp".to_owned()], 0, 0, 0, 0, 0);

        assert!(summary.set_written_file_count(2).is_err());
        assert_eq!(summary.written_file_count(), 0);
        assert!(summary.set_written_file_count(1).is_ok());
        assert_eq!(summary.written_file_count(), 1);
    }

    #[test]
    fn policy_status_is_derived_from_violations() {
        let summary = RefactorPreviewSummary::new(Vec::new(), 0, 0, 0, 0, 0);
        let options = RefactorPreviewPolicyOptions::new(true, false, false, None, None, None)
            .expect("valid options");
        let policy = evaluate_refactor_preview_policy(options, &summary);

        assert_eq!(policy.status(), RefactorPreviewPolicyStatus::Failed);
        assert!(!policy.passed());
        assert_eq!(policy.violations().len(), 1);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorPreviewDecisionStatus {
    BlockedByPolicy,
    RefusedUnparsableOutput,
    WriteApplied,
    DryRunReady,
}

impl RefactorPreviewDecisionStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "blocked-by-policy",
            Self::RefusedUnparsableOutput => "refused-unparsable-output",
            Self::WriteApplied => "write-applied",
            Self::DryRunReady => "dry-run-ready",
        }
    }

    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "preview-policy-failed",
            Self::RefusedUnparsableOutput => "rewritten-output-did-not-parse",
            Self::WriteApplied => "preview-write-applied",
            Self::DryRunReady => "all-dry-run-gates-passed",
        }
    }

    #[must_use]
    pub const fn next_action(self) -> &'static str {
        match self {
            Self::BlockedByPolicy => "review-policy-violations",
            Self::RefusedUnparsableOutput => "inspect-preview-parse-errors",
            Self::WriteApplied => "run-verification-or-review-diff",
            Self::DryRunReady => "review-preview-or-rerun-with-write",
        }
    }
}

#[must_use]
pub const fn decide_refactor_preview(
    write_requested: bool,
    policy_passed: bool,
    write_parse_refused: bool,
) -> RefactorPreviewDecisionStatus {
    if !policy_passed {
        RefactorPreviewDecisionStatus::BlockedByPolicy
    } else if write_parse_refused {
        RefactorPreviewDecisionStatus::RefusedUnparsableOutput
    } else if write_requested {
        RefactorPreviewDecisionStatus::WriteApplied
    } else {
        RefactorPreviewDecisionStatus::DryRunReady
    }
}
