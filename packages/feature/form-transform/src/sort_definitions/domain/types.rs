use std::path::PathBuf;

use paredit_core_syntax::definition::DefinitionCategory;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDefinitionsStrategy {
    Name,
    KindThenName,
}

impl SortDefinitionsStrategy {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::KindThenName => "kind-then-name",
        }
    }
}

#[derive(Debug)]
pub struct SortDefinitionsRequest<'a> {
    pub file: PathBuf,
    pub input: &'a str,
    pub dialect: Dialect,
    pub strategy: SortDefinitionsStrategy,
    pub write: bool,
}

#[derive(Debug)]
pub struct SortDefinitionsPlan {
    pub file: PathBuf,
    pub dialect: Dialect,
    pub strategy: SortDefinitionsStrategy,
    pub items: Vec<SortDefinitionsItem>,
    pub rewritten: String,
    pub changed: bool,
    pub written: bool,
}

#[derive(Debug, Clone)]
pub struct SortDefinitionsItem {
    pub old_path: Path,
    pub new_path: Path,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub source_index: usize,
    pub target_index: usize,
}

pub struct DefinitionBlock {
    pub start: usize,
    pub end: usize,
    pub entries: Vec<DefinitionEntry>,
}

/// `form_text` spans from the newline that ends the previous entry's line up
/// to this entry's own end, so a leading `;;` comment (or blank run) travels
/// with the definition below it when entries are reordered. The first entry
/// in the block has no previous entry to inherit trivia from, so its
/// `form_text` is just its own span and `has_leading_trivia` is `false`.
pub struct DefinitionEntry {
    pub item: SortDefinitionsItem,
    pub form_text: String,
    pub has_leading_trivia: bool,
}

pub struct RawDefinition {
    pub path: Path,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub source_index: usize,
}

pub struct BlockReplacement {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
