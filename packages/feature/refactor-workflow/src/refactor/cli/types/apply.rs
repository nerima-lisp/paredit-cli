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
    pub input_hash_matches: bool,
    pub output_hash_matches: bool,
    pub output_parse_ok: bool,
    pub expected_output_parse_ok: bool,
    pub manifest_flags_match: bool,
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
