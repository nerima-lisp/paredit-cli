use std::path::PathBuf;

use paredit_core_syntax::definition::DefinitionCategory;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path};

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
    pub destination: SplitFileDestination,
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
    pub destination: SplitFileDestination,
    pub changed: bool,
    pub written: bool,
}

/// What was already on disk where the split is writing.
///
/// The three states exclude the impossible combination of an existing file in
/// a missing directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitFileDestination {
    /// Neither the file nor its directory exists.
    MissingParent,
    /// The directory exists; the file does not.
    MissingFile,
    /// The file is already there.
    Existing,
}

impl SplitFileDestination {
    /// Rebuilds the state from the two observations the CLI makes.
    ///
    /// `file_exists && !parent_exists` is unobservable, so it folds into
    /// `Existing` rather than being rejected: the caller learned the file was
    /// readable, which settles the question.
    #[must_use]
    pub const fn observe(file_exists: bool, parent_exists: bool) -> Self {
        if file_exists {
            Self::Existing
        } else if parent_exists {
            Self::MissingFile
        } else {
            Self::MissingParent
        }
    }

    #[must_use]
    pub const fn to_file_existed(self) -> bool {
        matches!(self, Self::Existing)
    }

    #[must_use]
    pub const fn to_parent_existed(self) -> bool {
        matches!(self, Self::Existing | Self::MissingFile)
    }
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

#[cfg(test)]
mod destination_tests {
    use super::SplitFileDestination;

    /// The pair of booleans had four combinations and only three meanings: a
    /// file cannot exist inside a directory that does not. The enum has three
    /// states, so the fourth is no longer writable.
    #[test]
    fn the_impossible_combination_is_unrepresentable() {
        let existing = SplitFileDestination::observe(true, true);
        let missing_file = SplitFileDestination::observe(false, true);
        let missing_parent = SplitFileDestination::observe(false, false);

        assert_eq!(existing, SplitFileDestination::Existing);
        assert_eq!(missing_file, SplitFileDestination::MissingFile);
        assert_eq!(missing_parent, SplitFileDestination::MissingParent);

        // The fourth input folds into `Existing`: a readable file settles the
        // question of whether its directory is there.
        assert_eq!(
            SplitFileDestination::observe(true, false),
            SplitFileDestination::Existing
        );
    }

    /// The rendered fields are unchanged, which is what keeps the CLI's text
    /// and JSON output byte identical.
    #[test]
    fn the_two_rendered_fields_are_derived_unchanged() {
        for (state, file, parent) in [
            (SplitFileDestination::Existing, true, true),
            (SplitFileDestination::MissingFile, false, true),
            (SplitFileDestination::MissingParent, false, false),
        ] {
            assert_eq!(state.to_file_existed(), file, "{state:?}");
            assert_eq!(state.to_parent_existed(), parent, "{state:?}");
        }
    }
}
