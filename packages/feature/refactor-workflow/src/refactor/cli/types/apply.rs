use super::super::super::*;
use super::manifest::RefactorApplyManifestHeader;
use super::root::RefactorRootReport;

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplyResult {
    pub(in crate::presentation::cli) manifest: RefactorApplyManifestHeader,
    pub(in crate::presentation::cli) root: RefactorRootReport,
    pub(in crate::presentation::cli) write_requested: bool,
    pub(in crate::presentation::cli) manifest_policy_passed: bool,
    pub(in crate::presentation::cli) manifest_outputs_parse: bool,
    pub(in crate::presentation::cli) files: Vec<RefactorApplyFileResult>,
    pub(in crate::presentation::cli) summary: RefactorApplySummary,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplyFileResult {
    pub(in crate::presentation::cli) path: PathBuf,
    pub(in crate::presentation::cli) changed: bool,
    pub(in crate::presentation::cli) expected_changed: bool,
    pub(in crate::presentation::cli) written: bool,
    pub(in crate::presentation::cli) edit_count: usize,
    pub(in crate::presentation::cli) input_hash: String,
    pub(in crate::presentation::cli) output_hash: String,
    pub(in crate::presentation::cli) expected_input_hash: String,
    pub(in crate::presentation::cli) expected_output_hash: String,
    pub(in crate::presentation::cli) input_hash_matches: bool,
    pub(in crate::presentation::cli) output_hash_matches: bool,
    pub(in crate::presentation::cli) output_parse_ok: bool,
    pub(in crate::presentation::cli) expected_output_parse_ok: bool,
    pub(in crate::presentation::cli) manifest_flags_match: bool,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplySummary {
    pub(in crate::presentation::cli) file_count: usize,
    pub(in crate::presentation::cli) changed_file_count: usize,
    pub(in crate::presentation::cli) changed_files: Vec<String>,
    pub(in crate::presentation::cli) written_file_count: usize,
    pub(in crate::presentation::cli) edit_count: usize,
    pub(in crate::presentation::cli) stale_file_count: usize,
    pub(in crate::presentation::cli) output_hash_mismatch_count: usize,
    pub(in crate::presentation::cli) parse_error_count: usize,
    pub(in crate::presentation::cli) manifest_flag_mismatch_count: usize,
    pub(in crate::presentation::cli) applied: bool,
}
