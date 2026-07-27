use super::manifest::RefactorApplyManifestHeader;
use super::root::RefactorRootReport;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RefactorCheckResult {
    pub manifest: RefactorApplyManifestHeader,
    pub root: RefactorRootReport,
    pub manifest_policy_passed: bool,
    pub manifest_outputs_parse: bool,
    pub files: Vec<RefactorCheckFileResult>,
    pub summary: RefactorCheckSummary,
}

#[derive(Debug)]
pub struct RefactorCheckFileResult {
    pub path: PathBuf,
    pub changed: bool,
    pub expected_changed: bool,
    pub edit_count: usize,
    pub input_hash: String,
    pub output_hash: String,
    pub expected_input_hash: String,
    pub expected_output_hash: String,
    pub input_hash_matches: bool,
    pub output_hash_matches: bool,
    pub output_parse_ok: bool,
    pub expected_output_parse_ok: bool,
    pub manifest_flags_match: bool,
}

#[derive(Debug)]
pub struct RefactorCheckSummary {
    pub file_count: usize,
    pub changed_file_count: usize,
    pub changed_files: Vec<String>,
    pub edit_count: usize,
    pub stale_file_count: usize,
    pub output_hash_mismatch_count: usize,
    pub parse_error_count: usize,
    pub manifest_flag_mismatch_count: usize,
    pub can_apply: bool,
}

impl RefactorCheckFileResult {
    /// Whether the manifest was planned against a different input than the one
    /// on disk.
    ///
    /// Derived rather than stored: it is exactly `!input_hash_matches`, and a
    /// stored copy is a second source of truth that can disagree with the
    /// first. The architecture guide states this as a rule - derive
    /// presentation values at the serialization boundary instead of keeping
    /// them in the model.
    #[must_use]
    pub const fn stale(&self) -> bool {
        !self.input_hash_matches
    }
}
