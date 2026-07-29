//! Turning a per-file `RewritePlan` into the run-wide account of a replace.
//!
//! The account is deliberately four numbers rather than one. "12 rewritten" on
//! its own cannot be distinguished from "12 rewritten, 40 skipped", and the
//! second is a caller who should look again before writing.

use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{
    LineIndex, Pattern, SkipReason, Template, apply_plan, plan_rewrite,
};
use paredit_core_syntax::sexpr::SyntaxTree;

/// One planned edit, located for a human and for a follow-up command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedEdit {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub before: String,
    pub after: String,
}

/// One skipped match and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEdit {
    pub path: String,
    pub line: usize,
    pub reason: SkipReason,
}

/// Everything one file contributes to the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRewrite {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub edits: Vec<PlannedEdit>,
    pub skipped: Vec<SkippedEdit>,
    pub unchanged: usize,
    /// The whole file with every planned edit applied, or `None` when the plan
    /// is empty and there is nothing to write.
    pub rewritten: Option<String>,
}

impl FileRewrite {
    /// Whether this file would change.
    #[must_use]
    pub const fn is_touched(&self) -> bool {
        self.rewritten.is_some()
    }
}

/// Builds one file's contribution from the core plan.
#[must_use]
pub fn build_file_rewrite(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    pattern: &Pattern,
    template: &Template,
    allow_comment_loss: bool,
) -> FileRewrite {
    let plan = plan_rewrite(tree, pattern, template, dialect, allow_comment_loss);
    let source = tree.source();
    let index = LineIndex::new(source);
    let locate = |offset: usize| index.position_of(source, offset);

    let edits = plan
        .replacements
        .iter()
        .map(|replacement| {
            let position = locate(replacement.span.start().get());
            PlannedEdit {
                path: replacement.path.to_string(),
                line: position.line(),
                column: position.column(),
                before: replacement.before.clone(),
                after: replacement.after.clone(),
            }
        })
        .collect();
    let skipped = plan
        .skipped
        .iter()
        .map(|skipped| SkippedEdit {
            path: skipped.path.to_string(),
            line: locate(skipped.span.start().get()).line(),
            reason: skipped.reason,
        })
        .collect();

    let rewritten = (!plan.is_empty()).then(|| apply_plan(source, &plan));
    FileRewrite {
        path: path.to_path_buf(),
        dialect,
        edits,
        skipped,
        unchanged: plan.unchanged,
        rewritten,
    }
}

/// The run-wide tally, which is what the gate and the summary both read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RewriteTotals {
    pub files_scanned: usize,
    pub files_touched: usize,
    pub replacements: usize,
    pub skipped: usize,
    pub unchanged: usize,
}

impl RewriteTotals {
    /// Adds every file's contribution.
    #[must_use]
    pub fn of(files: &[FileRewrite]) -> Self {
        Self {
            files_scanned: files.len(),
            files_touched: files.iter().filter(|file| file.is_touched()).count(),
            replacements: files.iter().map(|file| file.edits.len()).sum(),
            skipped: files.iter().map(|file| file.skipped.len()).sum(),
            unchanged: files.iter().map(|file| file.unchanged).sum(),
        }
    }

    /// How many skipped matches carry `reason`, across the run.
    #[must_use]
    pub fn skipped_for(files: &[FileRewrite], reason: SkipReason) -> usize {
        files
            .iter()
            .flat_map(|file| &file.skipped)
            .filter(|skipped| skipped.reason == reason)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(source: &str, query: &str, template: &str) -> FileRewrite {
        let dialect = Dialect::CommonLisp;
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_file_rewrite(
            Path::new("a.lisp"),
            dialect,
            &tree,
            &Pattern::parse(query, dialect).expect("pattern"),
            &Template::parse(template, dialect).expect("template"),
            false,
        )
    }

    #[test]
    fn a_file_with_no_match_is_left_untouched_and_carries_no_rewrite() {
        let file = rewrite("(when a b)", "(if ?t ?a nil)", "(when ?t ?a)");
        assert!(!file.is_touched());
        assert!(file.rewritten.is_none());
    }

    #[test]
    fn a_planned_edit_reports_the_line_the_match_starts_on() {
        let file = rewrite("(f)\n(if a b nil)\n", "(if ?t ?a nil)", "(when ?t ?a)");
        assert_eq!(file.edits.len(), 1);
        assert_eq!(file.edits[0].line, 2);
        assert_eq!(file.rewritten.as_deref(), Some("(f)\n(when a b)\n"));
    }

    #[test]
    fn totals_separate_the_skipped_from_the_rewritten() {
        let files = vec![rewrite("(f (f x))", "(f ?x)", "(g ?x)")];
        let totals = RewriteTotals::of(&files);
        assert_eq!(totals.replacements, 1);
        assert_eq!(totals.skipped, 1);
        assert_eq!(
            RewriteTotals::skipped_for(&files, SkipReason::Overlapping),
            1
        );
    }

    #[test]
    fn a_rewrite_already_in_the_target_shape_counts_as_unchanged_not_as_absent() {
        let file = rewrite("(when a b)", "(when ?t ?a)", "(when ?t ?a)");
        assert_eq!(file.unchanged, 1);
        assert!(!file.is_touched());
    }
}
