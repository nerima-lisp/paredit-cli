use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteSpan;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RefactorApplyManifest {
    pub mode: String,
    pub from: String,
    pub to: String,
    pub policy_passed: bool,
    pub all_outputs_parse: bool,
    pub files: Vec<RefactorApplyManifestFile>,
}

#[derive(Debug)]
pub struct RefactorApplyManifestFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub changed: bool,
    pub output_parse_ok: bool,
    pub input_hash: String,
    pub output_hash: String,
    pub edits: Vec<RefactorApplyManifestEdit>,
}

#[derive(Debug)]
pub struct RefactorApplyManifestEdit {
    pub span: ByteSpan,
    pub replacement: String,
}

#[derive(Debug)]
pub struct RefactorApplyManifestHeader {
    pub path: PathBuf,
    pub hash: String,
    pub mode: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug)]
pub struct LoadedRefactorManifest {
    pub value: Value,
    pub hash: String,
}
