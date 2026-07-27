use super::super::super::*;

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplyManifest {
    pub(in crate::presentation::cli) mode: String,
    pub(in crate::presentation::cli) from: String,
    pub(in crate::presentation::cli) to: String,
    pub(in crate::presentation::cli) policy_passed: bool,
    pub(in crate::presentation::cli) all_outputs_parse: bool,
    pub(in crate::presentation::cli) files: Vec<RefactorApplyManifestFile>,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplyManifestFile {
    pub(in crate::presentation::cli) path: PathBuf,
    pub(in crate::presentation::cli) dialect: Dialect,
    pub(in crate::presentation::cli) changed: bool,
    pub(in crate::presentation::cli) output_parse_ok: bool,
    pub(in crate::presentation::cli) input_hash: String,
    pub(in crate::presentation::cli) output_hash: String,
    pub(in crate::presentation::cli) edits: Vec<RefactorApplyManifestEdit>,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplyManifestEdit {
    pub(in crate::presentation::cli) span: ByteSpan,
    pub(in crate::presentation::cli) replacement: String,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorApplyManifestHeader {
    pub(in crate::presentation::cli) path: PathBuf,
    pub(in crate::presentation::cli) hash: String,
    pub(in crate::presentation::cli) mode: String,
    pub(in crate::presentation::cli) from: String,
    pub(in crate::presentation::cli) to: String,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct LoadedRefactorManifest {
    pub(in crate::presentation::cli) value: Value,
    pub(in crate::presentation::cli) hash: String,
}
