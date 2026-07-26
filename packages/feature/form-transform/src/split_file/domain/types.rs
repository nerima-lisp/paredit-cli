use std::path::PathBuf;

use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path};

#[derive(Debug)]
pub struct SplitFileRequest<'a> {
    pub from_file: PathBuf,
    pub to_file: PathBuf,
    pub from_input: &'a str,
    pub to_input: &'a str,
    pub from_dialect: Dialect,
    pub to_dialect: Dialect,
    pub paths: Vec<Path>,
    pub names: Vec<String>,
    pub categories: Vec<DefinitionCategory>,
    pub to_file_existed: bool,
    pub to_parent_existed: bool,
    pub write: bool,
}

#[derive(Debug)]
pub struct SplitFilePlan {
    pub from_file: PathBuf,
    pub to_file: PathBuf,
    pub from_dialect: Dialect,
    pub to_dialect: Dialect,
    pub items: Vec<SplitFileItem>,
    pub from_rewritten: String,
    pub to_rewritten: String,
    pub to_file_existed: bool,
    pub to_parent_existed: bool,
    pub changed: bool,
    pub written: bool,
}

#[derive(Debug)]
pub struct SplitFileItem {
    pub path: Path,
    pub span: ByteSpan,
    pub removal_span: ByteSpan,
    pub definition: SplitFileDefinition,
    pub definition_text: String,
}

#[derive(Debug, Clone)]
pub struct SplitFileDefinition {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_count: Option<usize>,
    pub body_form_count: Option<usize>,
    pub package: Option<String>,
}
