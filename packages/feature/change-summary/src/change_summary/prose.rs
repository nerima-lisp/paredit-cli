//! Turning the facts into a sentence someone can paste into a pull request.
//!
//! Two rules govern every line here. It states what changed and never whether
//! the change was good — nothing produced by a tool that has not run the code
//! should read as approval. And it never says more than the comparison
//! established: "modified" means the text differs, not that the behaviour did.

use std::fmt::Display;

use paredit_core_cli::shared::terminal_safe;

use super::domain::{Change, ChangeSummary};

/// Escapes one interpolated value.
///
/// Applied to the values rather than to the finished draft: a definition name
/// comes from source and can carry a terminal escape, while the draft's own
/// newlines are structure. Escaping the whole document would turn its line
/// breaks into `\u{a}` and leave a pull request description on one line.
fn safe(value: impl Display) -> String {
    terminal_safe(value).to_string()
}

/// A one-line summary, suitable for a commit subject.
#[must_use]
pub fn headline(summary: &ChangeSummary) -> String {
    if summary.identical {
        return "No change: the two versions are identical.".to_owned();
    }
    if summary.formatting_only {
        return "Formatting only: no definition was added, removed, renamed, or changed."
            .to_owned();
    }

    let parts: Vec<String> = [
        ("added", summary.count("added")),
        ("removed", summary.count("removed")),
        ("renamed", summary.count("renamed")),
        ("modified", summary.count("modified")),
        ("moved", summary.count("moved")),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(kind, count)| format!("{count} {kind}"))
    .collect();

    format!(
        "{} {}.",
        join_with_and(&parts),
        if total(summary) == 1 {
            "definition"
        } else {
            "definitions"
        }
    )
}

fn total(summary: &ChangeSummary) -> usize {
    summary.changes.len()
}

/// The full draft: a headline, then one bullet per change.
#[must_use]
pub fn draft(summary: &ChangeSummary) -> String {
    let mut out = headline(summary);
    if summary.changes.is_empty() {
        out.push('\n');
        return out;
    }

    out.push_str("\n\n");
    for change in &summary.changes {
        out.push_str("- ");
        out.push_str(&describe(change));
        out.push('\n');
    }
    out
}

/// One change, as a sentence fragment.
#[must_use]
pub fn describe(change: &Change) -> String {
    match change {
        Change::Added(definition) => format!(
            "Added `{}` (line {}).",
            safe(definition.label()),
            definition.line
        ),
        Change::Removed(definition) => format!(
            "Removed `{}` (was line {}).",
            safe(definition.label()),
            definition.line
        ),
        Change::Modified { before, after } => format!(
            "Changed the body of `{}` (line {}{}).",
            safe(after.label()),
            after.line,
            if before.line == after.line {
                String::new()
            } else {
                format!(", was line {}", before.line)
            }
        ),
        Change::Renamed { before, after } => format!(
            "Renamed `{}` to `{}` (line {}); the body is unchanged.",
            safe(before.name.as_deref().unwrap_or("<unnamed>")),
            safe(after.name.as_deref().unwrap_or("<unnamed>")),
            after.line
        ),
        Change::Moved { before, after } => format!(
            "Moved `{}` from path {} to path {}; the body is unchanged.",
            safe(after.label()),
            safe(&before.path),
            safe(&after.path)
        ),
    }
}

fn join_with_and(parts: &[String]) -> String {
    match parts {
        [] => "No".to_owned(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::domain::summarise;
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn summary(before: &str, after: &str) -> ChangeSummary {
        summarise(before, after, Dialect::CommonLisp).expect("parses")
    }

    #[test]
    fn an_identical_pair_says_so_plainly() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) x)\n");
        assert_eq!(
            headline(&summary),
            "No change: the two versions are identical."
        );
        assert_eq!(
            draft(&summary),
            "No change: the two versions are identical.\n"
        );
    }

    /// The distinction a reviewer most wants made for them.
    #[test]
    fn a_formatting_only_edit_says_no_definition_changed() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) x)\n\n");
        assert_eq!(
            headline(&summary),
            "Formatting only: no definition was added, removed, renamed, or changed."
        );
    }

    #[test]
    fn one_change_is_singular() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) x)\n(defun g (y) y)\n");
        assert_eq!(headline(&summary), "1 added definition.");
    }

    #[test]
    fn several_kinds_are_joined_readably() {
        let before = "(defun a (x) x)\n(defun b (y) y)\n";
        let after = "(defun a (x) (list x))\n(defun c (z) z)\n";
        let headline = headline(&summary(before, after));
        assert!(headline.contains(" and "), "{headline}");
        assert!(headline.ends_with("definitions."), "{headline}");
    }

    #[test]
    fn a_rename_says_both_names_and_that_the_body_held() {
        let summary = summary("(defun old (x) (list x))\n", "(defun new (x) (list x))\n");
        let line = describe(&summary.changes[0]);
        assert!(line.contains("Renamed `old` to `new`"), "{line}");
        assert!(line.contains("body is unchanged"), "{line}");
    }

    #[test]
    fn a_modification_names_the_line_it_is_on() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) (list x))\n");
        assert_eq!(
            describe(&summary.changes[0]),
            "Changed the body of `defun f` (line 1)."
        );
    }

    #[test]
    fn a_move_names_both_paths() {
        let summary = summary(
            "(defun f (x) x)\n(defun g (y) y)\n",
            "(defun new (z) z)\n(defun f (x) x)\n(defun g (y) y)\n",
        );
        let moved = summary
            .changes
            .iter()
            .find(|change| change.kind() == "moved")
            .expect("a move");
        let line = describe(moved);
        assert!(line.contains("from path 0 to path 1"), "{line}");
    }

    #[test]
    fn the_draft_is_a_headline_and_one_bullet_per_change() {
        let before = "(defun a (x) x)\n(defun b (y) y)\n";
        let after = "(defun a (x) (list x))\n(defun b (y) (list y))\n";
        let draft = draft(&summary(before, after));
        let lines: Vec<&str> = draft.lines().filter(|line| !line.is_empty()).collect();

        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("- "));
        assert!(lines[2].starts_with("- "));
    }

    /// Nothing this tool produces should read as approval of a change it has
    /// not run.
    #[test]
    fn no_sentence_passes_judgement_on_the_change() {
        let before = "(defun a (x) x)\n(defun b (y) y)\n";
        let after = "(defun a (x) (list x))\n(defun c (z) z)\n";
        let draft = draft(&summary(before, after)).to_lowercase();

        for word in [
            "better", "worse", "improve", "cleaner", "should", "bug", "correct", "wrong",
        ] {
            assert!(
                !draft.contains(word),
                "the draft judges the change: {draft}"
            );
        }
    }
}
