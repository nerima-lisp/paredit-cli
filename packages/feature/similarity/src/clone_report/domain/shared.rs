//! Vocabulary the five clone reports share.

use std::path::{Path as FsPath, PathBuf};

use paredit_core_syntax::sexpr::ByteSpan;

use crate::similarity_report::domain::SimilarityFormReport;

/// Identity of one candidate form: where it is, not what it says.
///
/// A file path plus a byte span pins a form uniquely within a run, and unlike
/// the expression `Path` it stays comparable across the two halves of a pair
/// without borrowing the tree it came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormKey {
    path: PathBuf,
    start: usize,
    end: usize,
}

impl FormKey {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, span: ByteSpan) -> Self {
        Self {
            path: path.into(),
            start: span.start().get(),
            end: span.end().get(),
        }
    }

    #[must_use]
    pub fn of(form: &SimilarityFormReport) -> Self {
        Self::new(form.path().to_path_buf(), form.span())
    }

    #[must_use]
    pub fn path(&self) -> &FsPath {
        self.path.as_path()
    }

    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }
}

/// How many source lines a form's text occupies.
///
/// Counts the newlines the form itself contains, which is the number that
/// matters for "how much shorter would the file be" — a form that ends the line
/// it started on is one line whether or not a newline follows it.
#[must_use]
pub fn line_span_of(text: &str) -> usize {
    text.bytes().filter(|&byte| byte == b'\n').count() + 1
}
