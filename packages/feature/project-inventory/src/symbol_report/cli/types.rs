use std::path::PathBuf;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteSpan;

#[derive(Debug)]
pub struct SymbolReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub occurrences: Vec<SymbolReportOccurrence>,
}

#[derive(Debug)]
pub struct SymbolReportOccurrence {
    pub path: String,
    pub span: ByteSpan,
    pub context: Option<SymbolOccurrenceContext>,
}

#[derive(Debug)]
pub struct SymbolOccurrenceContext {
    pub path: String,
    pub span: ByteSpan,
    pub head: Option<String>,
    pub definition_like: bool,
}
