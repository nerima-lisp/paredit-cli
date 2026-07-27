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
    pub stale: bool,
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
