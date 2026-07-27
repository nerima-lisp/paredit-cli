use std::error::Error;
use std::fmt;

use crate::domain::mutation_safety::ReaderConditionalSafetyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameAtError {
    UnsupportedDialect,
    InvalidSelection,
    UnsupportedPackageSyntax,
    PackageQualifiedReference,
    NameConflict,
    InertReaderContext,
    ReaderConditional(ReaderConditionalSafetyError),
    Unresolved,
    Ambiguous,
}

impl fmt::Display for RenameAtError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedDialect => "rename-at currently supports Common Lisp only",
            Self::InvalidSelection => "--at must select a symbol atom",
            Self::UnsupportedPackageSyntax => {
                "rename-at does not support package-qualified, keyword, or uninterned symbols"
            }
            Self::PackageQualifiedReference => {
                "rename-at cannot safely rename a symbol referenced through a package qualifier"
            }
            Self::NameConflict => {
                "rename-at target conflicts with an existing binding in the same scope"
            }
            Self::InertReaderContext => "selected symbol is in quoted or quasiquoted data",
            Self::ReaderConditional(error) => return error.fmt(formatter),
            Self::Unresolved => "selected symbol does not resolve to a supported lexical binding",
            Self::Ambiguous => "selected symbol resolves to multiple supported lexical bindings",
        };
        formatter.write_str(message)
    }
}

impl Error for RenameAtError {}

impl From<ReaderConditionalSafetyError> for RenameAtError {
    fn from(error: ReaderConditionalSafetyError) -> Self {
        Self::ReaderConditional(error)
    }
}
