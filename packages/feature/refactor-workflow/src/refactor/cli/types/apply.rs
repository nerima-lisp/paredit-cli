use super::manifest::RefactorApplyManifestHeader;
use super::root::RefactorRootReport;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RefactorApplyResult {
    pub manifest: RefactorApplyManifestHeader,
    pub root: RefactorRootReport,
    pub write_requested: bool,
    pub manifest_policy_passed: bool,
    pub manifest_outputs_parse: bool,
    pub files: Vec<RefactorApplyFileResult>,
    pub summary: RefactorApplySummary,
}

#[derive(Debug)]
pub struct RefactorApplyFileResult {
    pub path: PathBuf,
    pub changed: bool,
    pub expected_changed: bool,
    pub written: bool,
    pub edit_count: usize,
    pub input_hash: String,
    pub output_hash: String,
    pub expected_input_hash: String,
    pub expected_output_hash: String,
    pub output_parse_ok: bool,
    pub expected_output_parse_ok: bool,
}

#[derive(Debug)]
pub struct RefactorApplySummary {
    pub file_count: usize,
    pub changed_file_count: usize,
    pub changed_files: Vec<String>,
    pub written_file_count: usize,
    pub edit_count: usize,
    pub stale_file_count: usize,
    pub output_hash_mismatch_count: usize,
    pub parse_error_count: usize,
    pub manifest_flag_mismatch_count: usize,
    pub applied: bool,
}

impl RefactorApplyFileResult {
    /// Whether the manifest was planned against a different input than the one
    /// on disk.
    ///
    /// Derived rather than stored: it is exactly `!input_hash_matches()`, and
    /// a stored copy is a second source of truth that can disagree with the
    /// first. The architecture guide states this as a rule - derive
    /// presentation values at the serialization boundary instead of keeping
    /// them in the model.
    #[must_use]
    pub fn stale(&self) -> bool {
        !self.input_hash_matches()
    }

    /// Whether the file on disk is the one the manifest was planned against.
    ///
    /// Derived from the two hashes this struct already holds. Storing it
    /// separately allowed the flag and the hashes to disagree - the same
    /// second-source-of-truth problem `stale` was written to avoid, which
    /// applied to three more fields than it fixed.
    #[must_use]
    pub fn input_hash_matches(&self) -> bool {
        self.input_hash == self.expected_input_hash
    }

    /// Whether re-applying the manifest's edits reproduced the planned output.
    #[must_use]
    pub fn output_hash_matches(&self) -> bool {
        self.output_hash == self.expected_output_hash
    }

    /// Whether the manifest's recorded flags still describe what happened.
    ///
    /// The manifest stores `changed` and `output_parse_ok` as claims; this
    /// re-derives both and compares. Storing the comparison as a third field
    /// meant a caller could see `manifest_flags_match: true` beside a
    /// `changed` that did not match `expected_changed`.
    #[must_use]
    pub const fn manifest_flags_match(&self) -> bool {
        self.changed == self.expected_changed
            && self.output_parse_ok == self.expected_output_parse_ok
    }
}
