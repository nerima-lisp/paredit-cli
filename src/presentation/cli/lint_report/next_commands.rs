//! What to run next, for `fix apply` and `fix plan` — and, since both reach
//! this workflow through the identical code path, for `inspect lint --fix`
//! and `inspect lint --fix-plan` as well.
//!
//! Mirrors `refactor-workflow`'s own `next_commands` module and
//! `paredit_core_cli::report::next_command`'s rule: a suggestion is only
//! emitted when the report's own contents justify it. Both functions here
//! always suggest the `fix`-namespace spelling regardless of which spelling
//! produced the report — the suggestion is new information this report is
//! adding, not a restatement of how the caller happened to invoke it, so the
//! two spellings still render byte-identical suggestions for byte-identical
//! runs.

use std::path::PathBuf;

use paredit_core_cli::report::next_command::{NextCommand, shell_path};

/// Joins `files` into one shell-safe argument list for a suggested command
/// line.
fn shell_files(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|file| shell_path(&file.display().to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Suggestion for a `fix apply` run (`lint_report_fix`).
///
/// Deliberately *not* keyed on the run's `fixes_deferred`/conflict count: the
/// fixed-point loop already re-offers a fix a same-pass overlap shadowed once
/// the form it was nested in has been rewritten, so a file the loop fully
/// converged on can still report a non-zero conflict count — suggesting a
/// follow-up there would point at a plan with nothing left in it, which is
/// exactly the "advertisement, not an answer" `next_command` exists to avoid.
///
/// `--no-destructive-fixes` is a real, load-bearing signal instead: it is a
/// caller's explicit request to leave a whole class of fixes unapplied, so
/// `skipped_destructive_count` names something this run is *certain* still
/// needs a decision, not an artifact of how the loop converged. `fix plan`
/// does not filter by that flag, so it is the command that shows exactly what
/// was left out.
#[must_use]
pub(super) fn fix_apply_next_commands(
    skipped_destructive_count: usize,
    files: &[PathBuf],
) -> Vec<NextCommand> {
    if skipped_destructive_count == 0 {
        return Vec::new();
    }
    vec![NextCommand::new(
        format!("paredit fix plan {} --output json", shell_files(files)),
        format!(
            "{skipped_destructive_count} destructive fix(es) were left unapplied by \
             --no-destructive-fixes; this shows what each one would do."
        ),
    )]
}

/// Suggestion for a `fix plan` run (`lint_report_fix_plan`).
///
/// `fix_count` is the plan's own entry count: an empty plan means there is
/// nothing an apply would change, so nothing is suggested.
#[must_use]
pub(super) fn fix_plan_next_commands(fix_count: usize, files: &[PathBuf]) -> Vec<NextCommand> {
    if fix_count == 0 {
        return Vec::new();
    }
    vec![NextCommand::new(
        format!("paredit fix apply {} --output json", shell_files(files)),
        format!("{fix_count} fixable finding(s) found; this applies them."),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_skipped_destructive_fixes_suggests_nothing() {
        assert!(fix_apply_next_commands(0, &[PathBuf::from("a.lisp")]).is_empty());
    }

    #[test]
    fn skipped_destructive_fixes_suggest_fix_plan_over_the_same_files() {
        let commands =
            fix_apply_next_commands(2, &[PathBuf::from("a.lisp"), PathBuf::from("b.lisp")]);
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command,
            "paredit fix plan a.lisp b.lisp --output json"
        );
        assert!(commands[0].why.contains('2'));
    }

    #[test]
    fn an_empty_plan_suggests_nothing() {
        assert!(fix_plan_next_commands(0, &[PathBuf::from("a.lisp")]).is_empty());
    }

    #[test]
    fn a_non_empty_plan_suggests_fix_apply_over_the_same_files() {
        let commands = fix_plan_next_commands(3, &[PathBuf::from("a.lisp")]);
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command,
            "paredit fix apply a.lisp --output json"
        );
        assert!(commands[0].why.contains('3'));
    }

    /// A path needing quoting must still be runnable as a single argument.
    #[test]
    fn a_path_needing_quoting_is_quoted_in_the_suggestion() {
        let commands = fix_apply_next_commands(1, &[PathBuf::from("my files/a.lisp")]);
        assert_eq!(
            commands[0].command,
            "paredit fix plan 'my files/a.lisp' --output json"
        );
    }
}
