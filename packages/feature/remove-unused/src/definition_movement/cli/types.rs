use paredit_core_cli::args::MoveInsert;
use std::path::PathBuf;

use crate::definition_report::usecase::DefinitionReportItem;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path};

#[derive(Debug)]
pub struct MoveDefinitionPlan {
    pub from_file: PathBuf,
    pub to_file: PathBuf,
    pub from_dialect: Dialect,
    pub to_dialect: Dialect,
    pub path: Path,
    pub span: ByteSpan,
    pub definition: DefinitionReportItem,
    pub definition_text: String,
    pub from_rewritten: String,
    pub to_rewritten: String,
    pub to_file_existed: bool,
    pub changed: bool,
    pub written: bool,
}

#[derive(Debug)]
pub struct MoveFormPlan {
    pub from_file: PathBuf,
    pub to_file: PathBuf,
    pub from_dialect: Dialect,
    pub to_dialect: Dialect,
    pub path: Path,
    pub span: ByteSpan,
    pub head: Option<String>,
    pub form_text: String,
    pub insert: MoveInsert,
    pub anchor_path: Option<Path>,
    pub anchor_span: Option<ByteSpan>,
    pub from_rewritten: String,
    pub to_rewritten: String,
    pub to_file_existed: bool,
    pub changed: bool,
    pub written: bool,
}
