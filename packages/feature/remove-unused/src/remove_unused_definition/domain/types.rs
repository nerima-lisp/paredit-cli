use std::path::PathBuf;

use paredit_core_syntax::definition::DefinitionCategory;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{AtomOccurrence, ByteSpan, ExpressionView};
use paredit_feature_package::package_report::domain::PackageDefinitionReport;

#[derive(Debug, Clone)]
pub struct RemoveUnusedDefinitionInputFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub definitions: Vec<UnusedDefinitionDefinition>,
    pub atoms: Vec<AtomOccurrence>,
    pub text: String,
    /// The root view from the parse this file was already loaded with.
    ///
    /// Kept alongside `text` rather than instead of it: `text` is still read
    /// directly for substring reference-needle scans, but this is what saves
    /// `collect_unused_definition_candidates` from re-parsing `text` from
    /// scratch just to get a view it already had once.
    pub root_view: ExpressionView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedDefinitionDefinition {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_count: Option<usize>,
    pub body_form_count: Option<usize>,
    pub package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RemoveUnusedDefinitionsRequest {
    pub files: Vec<RemoveUnusedDefinitionInputFile>,
    pub package_definitions: Vec<PackageDefinitionReport>,
    pub include_protected: bool,
    pub include_exported: bool,
}

#[derive(Debug, Clone)]
pub struct RemoveUnusedDefinitionsPlan {
    pub files: Vec<RemoveUnusedDefinitionsFilePlan>,
    pub candidate_count: usize,
    pub removal_count: usize,
    pub skipped_count: usize,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct RemoveUnusedDefinitionsFilePlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub rewritten: String,
    pub changed: bool,
    pub removals: Vec<PlannedDefinitionRemoval>,
    pub skipped: Vec<SkippedDefinitionRemoval>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDefinitionRemoval {
    pub definition: UnusedDefinitionDefinition,
    pub definition_text: String,
    pub removal_span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedDefinitionRemoval {
    pub definition: UnusedDefinitionDefinition,
    pub reason: SkippedDefinitionRemovalReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkippedDefinitionRemovalReason {
    ExportedDefinition,
    ProtectedDefinitionCategory,
}

impl SkippedDefinitionRemovalReason {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ExportedDefinition => "exported-definition",
            Self::ProtectedDefinitionCategory => "protected-definition-category",
        }
    }
}
