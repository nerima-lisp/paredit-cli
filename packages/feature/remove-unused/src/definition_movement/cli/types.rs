use std::path::PathBuf;

use crate::application::usecase::definition_report::DefinitionReportItem;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path};

use super::super::MoveInsert;

#[derive(Debug)]
pub(super) struct MoveDefinitionPlan {
    pub(super) from_file: PathBuf,
    pub(super) to_file: PathBuf,
    pub(super) from_dialect: Dialect,
    pub(super) to_dialect: Dialect,
    pub(super) path: Path,
    pub(super) span: ByteSpan,
    pub(super) definition: DefinitionReportItem,
    pub(super) definition_text: String,
    pub(super) from_rewritten: String,
    pub(super) to_rewritten: String,
    pub(super) to_file_existed: bool,
    pub(super) changed: bool,
    pub(super) written: bool,
}

#[derive(Debug)]
pub(super) struct MoveFormPlan {
    pub(super) from_file: PathBuf,
    pub(super) to_file: PathBuf,
    pub(super) from_dialect: Dialect,
    pub(super) to_dialect: Dialect,
    pub(super) path: Path,
    pub(super) span: ByteSpan,
    pub(super) head: Option<String>,
    pub(super) form_text: String,
    pub(super) insert: MoveInsert,
    pub(super) anchor_path: Option<Path>,
    pub(super) anchor_span: Option<ByteSpan>,
    pub(super) from_rewritten: String,
    pub(super) to_rewritten: String,
    pub(super) to_file_existed: bool,
    pub(super) changed: bool,
    pub(super) written: bool,
}
