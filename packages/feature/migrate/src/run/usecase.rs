//! Applying a recipe's steps to one file, in order.
//!
//! # Why the steps re-parse between each other
//!
//! A recipe is *ordered*, and the order only means something if step 2 sees
//! what step 1 produced. `nil-conditionals` relies on exactly that: its first
//! step turns `(if (not p) a nil)` into `(unless p a)` so that its second step
//! — the general `(if ?t ?a nil)` — no longer matches it. Planning both steps
//! against the original tree would let both claim the same form, and the
//! second would win by being applied last.
//!
//! Re-parsing costs one parse per step per file, and a recipe is not always a
//! handful of steps: `elisp-cl-lib` has fifty. Over Emacs's own `lisp/` tree
//! that was 45 seconds to produce 77 replacements, with 37% of the time spent
//! materializing and dropping syntax trees for steps that could not match.
//!
//! So a step whose literal atoms do not all appear in the text is skipped
//! before anything is parsed. The test is sound in the only direction that
//! matters — a substring miss proves the pattern cannot match, while a hit
//! proves nothing and falls through to the real matcher. On that corpus it
//! skips 91% of step-instances.

use std::path::{Path, PathBuf};

use paredit_core_cli::CliResult;

use crate::error::MigrateError;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{
    Pattern, RewriteAllowances, SkipReason, Template, apply_plan, plan_rewrite,
};
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::recipe::Migration;

/// What one step did to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    pub query: String,
    pub rewrite: String,
    pub replacements: usize,
    pub skipped: usize,
}

/// What the whole recipe did to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOutcome {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Per step, in recipe order. Empty when the file was out of scope.
    pub steps: Vec<StepOutcome>,
    /// Set when the recipe's `:dialects` excluded this file.
    pub out_of_scope: bool,
    /// The file with every step applied, or `None` when nothing changed.
    pub rewritten: Option<String>,
}

impl FileOutcome {
    /// Whether this file would change.
    #[must_use]
    pub const fn is_touched(&self) -> bool {
        self.rewritten.is_some()
    }

    /// Every step's replacements added up.
    #[must_use]
    pub fn replacements(&self) -> usize {
        self.steps.iter().map(|step| step.replacements).sum()
    }

    /// Every step's skipped matches added up.
    #[must_use]
    pub fn skipped(&self) -> usize {
        self.steps.iter().map(|step| step.skipped).sum()
    }
}

/// Runs `migration` over one file's text.
///
/// `dialect` is the file's own dialect, which is both what the steps are
/// parsed with and what the scope check tests. A file outside the recipe's
/// `:dialects` is returned untouched with `out_of_scope` set, rather than
/// omitted: "this recipe skipped 400 of your 412 files" is the answer to why
/// a run that looked successful changed nothing.
pub fn run_migration(
    path: &Path,
    dialect: Dialect,
    source: &str,
    migration: &Migration,
    allow: RewriteAllowances,
) -> CliResult<FileOutcome> {
    if !migration.covers(dialect) {
        return Ok(FileOutcome {
            path: path.to_path_buf(),
            dialect,
            steps: Vec::new(),
            out_of_scope: true,
            rewritten: None,
        });
    }

    let mut current = source.to_owned();
    let mut steps = Vec::with_capacity(migration.steps.len());

    for (index, step) in migration.steps.iter().enumerate() {
        let pattern = Pattern::parse(&step.query, dialect).map_err(MigrateError::step(
            &migration.name,
            index,
            ":query",
        ))?;
        let template = Template::parse(&step.rewrite, dialect).map_err(MigrateError::step(
            &migration.name,
            index,
            ":rewrite",
        ))?;
        template
            .check_against(&pattern)
            .map_err(MigrateError::step(&migration.name, index, ""))?;

        if !may_match(&pattern, &current, dialect) {
            steps.push(StepOutcome {
                query: step.query.clone(),
                rewrite: step.rewrite.clone(),
                replacements: 0,
                skipped: 0,
            });
            continue;
        }

        // The write guard, per step: a migration that leaves a file it
        // cannot read back has broken it, and the next step would compound it.
        let tree = SyntaxTree::parse_with_dialect(&current, dialect).map_err(|source| {
            MigrateError::StepLeftFileUnparseable {
                migration: migration.name.clone(),
                index,
                path: path.display().to_string(),
                source,
            }
        })?;
        let plan = plan_rewrite(&tree, &pattern, &template, dialect, allow);
        steps.push(StepOutcome {
            query: step.query.clone(),
            rewrite: step.rewrite.clone(),
            replacements: plan.replacements.len(),
            skipped: plan.skipped.len(),
        });
        if !plan.is_empty() {
            current = apply_plan(tree.source(), &plan);
        }
    }

    let changed = current != source;
    Ok(FileOutcome {
        path: path.to_path_buf(),
        dialect,
        steps,
        out_of_scope: false,
        rewritten: changed.then_some(current),
    })
}

/// Whether `pattern`'s literal atoms all appear in `source`.
///
/// A cheap, one-directional prefilter: `false` proves the pattern cannot
/// match, `true` proves nothing. Case-insensitive for a dialect whose reader
/// folds case, because the matcher folds too — a case-sensitive test would
/// skip a Common Lisp step over a file spelling the operator `DEFUN`.
fn may_match(pattern: &Pattern, source: &str, dialect: Dialect) -> bool {
    let literals = pattern.required_literals();
    if literals.is_empty() {
        return true;
    }
    if dialect != Dialect::CommonLisp {
        return literals.iter().all(|literal| source.contains(literal));
    }
    // Common Lisp's reader folds case *and* package qualifiers, and the
    // matcher folds with it: a literal `cl:quote` matches a source `quote`,
    // and `defun` matches `DEFUN`. Both have to be folded out of the test, or
    // it skips a step that would have matched — a prefilter is only allowed to
    // be wrong in the direction that costs time, never in the one that costs
    // correctness.
    let folded = source.to_ascii_lowercase();
    literals.iter().all(|literal| {
        let bare = literal.rsplit(':').next().unwrap_or(literal);
        folded.contains(&bare.to_ascii_lowercase())
    })
}

/// The run-wide tally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MigrationTotals {
    pub files_scanned: usize,
    pub files_out_of_scope: usize,
    pub files_touched: usize,
    pub replacements: usize,
    pub skipped: usize,
}

impl MigrationTotals {
    /// Adds every file's contribution, borrowed rather than cloned.
    #[must_use]
    pub fn of(files: &[&FileOutcome]) -> Self {
        Self {
            files_scanned: files.len(),
            files_out_of_scope: files.iter().filter(|file| file.out_of_scope).count(),
            files_touched: files.iter().filter(|file| file.is_touched()).count(),
            replacements: files.iter().map(|file| file.replacements()).sum(),
            skipped: files.iter().map(|file| file.skipped()).sum(),
        }
    }
}

/// The skip reasons, listed so a report can print the zeroes too.
pub const SKIP_REASONS: [SkipReason; 3] = [
    SkipReason::Overlapping,
    SkipReason::CommentLoss,
    SkipReason::Quoted,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::parse_recipes;

    fn recipe(text: &str) -> Migration {
        parse_recipes(text, "test").expect("parse").remove(0)
    }

    fn nil_conditionals() -> Migration {
        recipe(
            r#"(defmigration nil-conditionals
                 :description "d"
                 :dialects (common-lisp)
                 :steps ((:query (if (not ?t) ?a nil) :rewrite (unless ?t ?a))
                         (:query (if ?t ?a nil) :rewrite (when ?t ?a))))"#,
        )
    }

    #[test]
    fn an_earlier_step_wins_the_form_a_later_step_would_also_have_claimed() {
        let outcome = run_migration(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            "(if (not p) a nil)",
            &nil_conditionals(),
            RewriteAllowances::default(),
        )
        .expect("run");
        assert_eq!(outcome.rewritten.as_deref(), Some("(unless p a)"));
        assert_eq!(outcome.steps[0].replacements, 1);
        assert_eq!(outcome.steps[1].replacements, 0);
    }

    #[test]
    fn a_form_only_the_later_step_matches_still_reaches_it() {
        let outcome = run_migration(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            "(if p a nil)",
            &nil_conditionals(),
            RewriteAllowances::default(),
        )
        .expect("run");
        assert_eq!(outcome.rewritten.as_deref(), Some("(when p a)"));
    }

    #[test]
    fn a_file_outside_the_dialect_scope_is_reported_rather_than_rewritten() {
        let outcome = run_migration(
            Path::new("a.el"),
            Dialect::EmacsLisp,
            "(if p a nil)",
            &nil_conditionals(),
            RewriteAllowances::default(),
        )
        .expect("run");
        assert!(outcome.out_of_scope);
        assert!(!outcome.is_touched());
        assert!(outcome.steps.is_empty());
    }

    #[test]
    fn a_file_no_step_matches_is_left_byte_identical() {
        let outcome = run_migration(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            "(when p a)",
            &nil_conditionals(),
            RewriteAllowances::default(),
        )
        .expect("run");
        assert!(!outcome.is_touched());
        assert_eq!(outcome.replacements(), 0);
    }

    #[test]
    fn running_a_recipe_twice_changes_nothing_the_second_time() {
        let once = run_migration(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            "(if p a nil)\n(if (not q) b nil)\n",
            &nil_conditionals(),
            RewriteAllowances::default(),
        )
        .expect("run")
        .rewritten
        .expect("changed");
        let twice = run_migration(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &once,
            &nil_conditionals(),
            RewriteAllowances::default(),
        )
        .expect("run");
        assert!(!twice.is_touched(), "recipe is not idempotent: {once}");
    }

    #[test]
    fn the_cl_lib_recipe_prefixes_a_call_and_leaves_a_quoted_symbol_alone() {
        let entries = crate::catalog::built_in().expect("built-ins");
        let migration = crate::catalog::find(&entries, "elisp-cl-lib")
            .expect("shipped")
            .migration;
        let outcome = run_migration(
            Path::new("a.el"),
            Dialect::EmacsLisp,
            "(incf counter)\n(funcall 'incf)\n",
            &migration,
            RewriteAllowances::default(),
        )
        .expect("run");
        assert_eq!(
            outcome.rewritten.as_deref(),
            Some("(cl-incf counter)\n(funcall 'incf)\n")
        );
    }
}
