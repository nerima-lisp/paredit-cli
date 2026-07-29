//! `query count` — several patterns, side by side, over one file set.
//!
//! Not a `--output`-only variation on `query find`. `find` answers "where",
//! and its shape is one row per match; `count` answers "how many, and how does
//! that compare", and its shape is a matrix of pattern against file. Rendering
//! a matrix through a finding list would mean either one row per zero (a
//! report mostly made of absence) or dropping the zeros (which is the number
//! the question was about).

use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{Pattern, match_all};
use paredit_core_syntax::sexpr::SyntaxTree;

/// One pattern as the caller wrote it, with its parsed form.
#[derive(Debug)]
pub struct CountedPattern {
    pub text: String,
    pub pattern: Pattern,
}

/// One file's tally, one entry per pattern in command-line order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTally {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub counts: Vec<usize>,
}

impl FileTally {
    /// Whether every pattern missed this file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.counts.iter().all(|count| *count == 0)
    }
}

/// The whole tally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountReport {
    /// The patterns as written, so the matrix columns can be labelled.
    pub patterns: Vec<String>,
    pub files: Vec<FileTally>,
    /// Column totals, in the same order as `patterns`.
    pub totals: Vec<usize>,
}

impl CountReport {
    /// Every pattern's matches added together.
    #[must_use]
    pub fn grand_total(&self) -> usize {
        self.totals.iter().sum()
    }
}

/// Counts each pattern's matches in one file.
#[must_use]
pub fn tally_file(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    patterns: &[CountedPattern],
) -> FileTally {
    FileTally {
        path: path.to_path_buf(),
        dialect,
        counts: patterns
            .iter()
            .map(|counted| match_all(tree, &counted.pattern, dialect).len())
            .collect(),
    }
}

/// Assembles the per-file tallies into the report, with column totals.
#[must_use]
pub fn build_count_report(patterns: &[CountedPattern], files: Vec<FileTally>) -> CountReport {
    let mut totals = vec![0usize; patterns.len()];
    for file in &files {
        for (total, count) in totals.iter_mut().zip(&file.counts) {
            *total += count;
        }
    }
    CountReport {
        patterns: patterns
            .iter()
            .map(|counted| counted.text.clone())
            .collect(),
        files,
        totals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(text: &str) -> CountedPattern {
        CountedPattern {
            text: text.to_owned(),
            pattern: Pattern::parse(text, Dialect::CommonLisp).expect("pattern"),
        }
    }

    #[test]
    fn totals_add_one_column_at_a_time_across_files() {
        let patterns = [counted("(defun ?n ...)"), counted("(if ?t ?a nil)")];
        let files = vec![
            FileTally {
                path: PathBuf::from("a.lisp"),
                dialect: Dialect::CommonLisp,
                counts: vec![2, 0],
            },
            FileTally {
                path: PathBuf::from("b.lisp"),
                dialect: Dialect::CommonLisp,
                counts: vec![1, 3],
            },
        ];
        let report = build_count_report(&patterns, files);
        assert_eq!(report.totals, vec![3, 3]);
        assert_eq!(report.grand_total(), 6);
    }

    #[test]
    fn a_file_no_pattern_reaches_is_reported_as_empty() {
        let tally = FileTally {
            path: PathBuf::from("a.lisp"),
            dialect: Dialect::CommonLisp,
            counts: vec![0, 0],
        };
        assert!(tally.is_empty());
    }

    #[test]
    fn counting_reports_every_nested_match_the_matcher_finds() {
        let dialect = Dialect::CommonLisp;
        let tree = SyntaxTree::parse_with_dialect("(let () (let () x))", dialect).expect("parse");
        let patterns = [counted("(let () ...)")];
        let tally = tally_file(Path::new("a.lisp"), dialect, &tree, &patterns);
        assert_eq!(tally.counts, vec![2]);
    }
}
