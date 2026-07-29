//! Comparing two versions of a document, in terms a reviewer uses.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{ExpressionKind, SyntaxTree};

/// One top-level form, as this comparison sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// The operator: `defun`, `defclass`, `defpackage`.
    pub head: String,
    /// The name it binds, when the second child is an atom.
    pub name: Option<String>,
    /// Dot-separated child indexes.
    pub path: String,
    /// The form's source text, exactly.
    pub text: String,
    /// The form re-rendered from its tree, with every run of whitespace
    /// normalised away.
    ///
    /// This is what "did the definition change" is decided on. Comparing the
    /// raw text would report a reindent as a change to the definition, which
    /// is exactly the distinction a reviewer wants made for them.
    pub shape: String,
    /// 1-based line the form starts on.
    pub line: usize,
    /// Where the name sits inside `text`, as a byte range.
    ///
    /// Recorded rather than searched for. Replacing the *first* occurrence of
    /// the name in the form's text is wrong the moment the name's letters
    /// appear earlier: `(defun d ...)` would blank the `d` in `defun`, and the
    /// rename detection built on it would silently stop working for every
    /// definition named after a letter of its own operator.
    pub name_range: Option<(usize, usize)>,
}

impl Definition {
    /// What this form is called, for a sentence.
    #[must_use]
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => format!("{} {}", self.head, name),
            None => format!("{} at path {}", self.head, self.path),
        }
    }

    /// The identity two versions of the same definition share.
    ///
    /// Not the path: a path shifts whenever anything above it grows, and a
    /// comparison keyed on one reports the whole rest of the file as changed
    /// every time a definition is inserted.
    ///
    /// A form with no name — `(progn ...)`, an immediately applied lambda —
    /// falls back to its own text. There is nothing else to identify it by,
    /// and letting every unnamed form share one identity would silently pair
    /// the first `progn` in one version with the first in the other however
    /// unrelated they are.
    #[must_use]
    pub fn identity(&self) -> String {
        match &self.name {
            Some(name) => format!("{}\u{1}{name}", self.head),
            None => format!("{}\u{1}\u{3}{}", self.head, self.shape),
        }
    }

    /// The body with the name replaced, so two definitions that differ only in
    /// what they are called compare equal.
    ///
    /// The comparison is exact on everything but the name, which is what makes
    /// a match trustworthy enough to call a rename. It runs over the
    /// normalised shape, so a rename that also reindented is still a rename.
    #[must_use]
    pub fn anonymised(&self) -> String {
        let Some(name) = self.name.as_deref() else {
            return self.shape.clone();
        };
        // The name is the shape's second token, and the shape has exactly one
        // space between tokens, so its position is unambiguous — unlike in the
        // raw text, where `(defun d ...)` has a `d` in `defun` first.
        let head_end = self
            .shape
            .find(' ')
            .map_or(self.shape.len(), |index| index + 1);
        let (prefix, rest) = self.shape.split_at(head_end);
        match rest.strip_prefix(name) {
            Some(tail) => format!("{prefix}\u{2}{tail}"),
            None => self.shape.clone(),
        }
    }
}

/// One thing that happened between the two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(Definition),
    Removed(Definition),
    /// Same identity, different text.
    Modified {
        before: Definition,
        after: Definition,
    },
    /// Same body, different name.
    Renamed {
        before: Definition,
        after: Definition,
    },
    /// Same identity and same text, different path.
    Moved {
        before: Definition,
        after: Definition,
    },
}

impl Change {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Added(_) => "added",
            Self::Removed(_) => "removed",
            Self::Modified { .. } => "modified",
            Self::Renamed { .. } => "renamed",
            Self::Moved { .. } => "moved",
        }
    }

    /// The definition to sort and report this change under.
    #[must_use]
    pub const fn anchor(&self) -> &Definition {
        match self {
            Self::Added(definition) | Self::Removed(definition) => definition,
            Self::Modified { after, .. }
            | Self::Renamed { after, .. }
            | Self::Moved { after, .. } => after,
        }
    }
}

/// Everything the comparison found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSummary {
    pub changes: Vec<Change>,
    /// The two versions' text is identical.
    pub identical: bool,
    /// The text differs but no definition did: whitespace, comments, or a
    /// reordering the structure does not see.
    pub formatting_only: bool,
    pub before_definitions: usize,
    pub after_definitions: usize,
}

impl ChangeSummary {
    #[must_use]
    pub fn count(&self, kind: &str) -> usize {
        self.changes
            .iter()
            .filter(|change| change.kind() == kind)
            .count()
    }
}

/// Compares two versions of one document.
#[must_use]
pub fn summarise(before: &str, after: &str, dialect: Dialect) -> Option<ChangeSummary> {
    let before_tree = SyntaxTree::parse_with_dialect(before, dialect).ok()?;
    let after_tree = SyntaxTree::parse_with_dialect(after, dialect).ok()?;

    let before_definitions = collect(&before_tree, before, dialect);
    let after_definitions = collect(&after_tree, after, dialect);
    let changes = compare(&before_definitions, &after_definitions);

    Some(ChangeSummary {
        identical: before == after,
        formatting_only: before != after && changes.is_empty(),
        before_definitions: before_definitions.len(),
        after_definitions: after_definitions.len(),
        changes,
    })
}

fn collect(tree: &SyntaxTree, source: &str, dialect: Dialect) -> Vec<Definition> {
    tree.outline(|head| dialect.is_definition_head(head))
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path.to_string();
            let view = tree.select_path(&entry.path).ok()?.view();
            let named = view
                .children
                .get(1)
                .filter(|child| child.kind == ExpressionKind::Atom)
                .and_then(|child| child.text.clone().map(|text| (text, child.span)));
            let range = entry.span.as_range();
            let name_range = named.as_ref().map(|(_, span)| {
                (
                    span.start().get() - range.start,
                    span.end().get() - range.start,
                )
            });
            Some(Definition {
                head: entry.head.unwrap_or_else(|| "<list>".to_owned()),
                name: named.map(|(text, _)| text),
                path,
                text: source.get(range.clone()).unwrap_or_default().to_owned(),
                shape: render_expression(&view),
                line: source[..range.start].matches('\n').count() + 1,
                name_range,
            })
        })
        .collect()
}

fn compare(before: &[Definition], after: &[Definition]) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut unmatched_before: Vec<&Definition> = Vec::new();
    let mut unmatched_after: Vec<&Definition> = Vec::new();

    for old in before {
        match after.iter().find(|new| new.identity() == old.identity()) {
            Some(new) if new.shape != old.shape => changes.push(Change::Modified {
                before: old.clone(),
                after: new.clone(),
            }),
            Some(new) if new.path != old.path => changes.push(Change::Moved {
                before: old.clone(),
                after: new.clone(),
            }),
            Some(_) => {}
            None => unmatched_before.push(old),
        }
    }
    for new in after {
        if !before.iter().any(|old| old.identity() == new.identity()) {
            unmatched_after.push(new);
        }
    }

    // A removal and an addition whose bodies match once the name is set aside
    // is a rename. Reported only on an exact match: a confidently wrong
    // summary in a pull request costs more than a vague one.
    let mut renamed_after: Vec<usize> = Vec::new();
    for old in &unmatched_before {
        let paired = unmatched_after.iter().enumerate().find(|(index, new)| {
            !renamed_after.contains(index)
                && new.head == old.head
                && new.name.is_some()
                && old.name.is_some()
                && new.anonymised() == old.anonymised()
        });
        match paired {
            Some((index, new)) => {
                renamed_after.push(index);
                changes.push(Change::Renamed {
                    before: (*old).clone(),
                    after: (*new).clone(),
                });
            }
            None => changes.push(Change::Removed((*old).clone())),
        }
    }
    for (index, new) in unmatched_after.iter().enumerate() {
        if !renamed_after.contains(&index) {
            changes.push(Change::Added((*new).clone()));
        }
    }

    // Source order, so the summary reads down the file rather than by
    // whichever pass happened to find each change.
    changes.sort_by(|left, right| {
        left.anchor()
            .line
            .cmp(&right.anchor().line)
            .then_with(|| left.anchor().label().cmp(&right.anchor().label()))
    });
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    const CL: Dialect = Dialect::CommonLisp;

    fn summary(before: &str, after: &str) -> ChangeSummary {
        summarise(before, after, CL).expect("both versions parse")
    }

    #[test]
    fn an_unchanged_document_reports_nothing() {
        let source = "(defun f (x) x)\n";
        let summary = summary(source, source);
        assert!(summary.identical);
        assert!(!summary.formatting_only);
        assert!(summary.changes.is_empty());
    }

    /// The distinction that decides whether a review needs attention.
    #[test]
    fn a_whitespace_only_edit_is_formatting_rather_than_a_change() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) x)\n\n\n");
        assert!(!summary.identical);
        assert!(summary.formatting_only);
        assert!(summary.changes.is_empty());
    }

    /// Whitespace *inside* a definition counts too. Comparing raw text would
    /// report a reindent as a change to the definition, which is exactly the
    /// distinction this command exists to make.
    #[test]
    fn reindenting_a_definition_is_formatting_rather_than_a_change() {
        let summary = summary(
            "(defun f (x) (list x))\n",
            "(defun f (x)\n  (list\n    x))\n",
        );
        assert!(summary.formatting_only, "{:?}", summary.changes);
    }

    /// And a rename that also reindented is still a rename, because the
    /// comparison runs over the normalised shape.
    #[test]
    fn a_rename_that_also_reindented_is_still_a_rename() {
        let summary = summary(
            "(defun old (x) (list x))\n",
            "(defun new (x)\n  (list x))\n",
        );
        assert_eq!(summary.count("renamed"), 1, "{:?}", summary.changes);
    }

    #[test]
    fn an_added_definition_is_reported_once() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) x)\n(defun g (y) y)\n");
        assert_eq!(summary.changes.len(), 1);
        assert_eq!(summary.changes[0].kind(), "added");
        assert_eq!(summary.changes[0].anchor().label(), "defun g");
        assert_eq!(summary.after_definitions, 2);
    }

    #[test]
    fn a_removed_definition_is_reported_once() {
        let summary = summary("(defun f (x) x)\n(defun g (y) y)\n", "(defun f (x) x)\n");
        assert_eq!(summary.changes.len(), 1);
        assert_eq!(summary.changes[0].kind(), "removed");
    }

    #[test]
    fn a_changed_body_is_a_modification_rather_than_a_replacement() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) (list x))\n");
        assert_eq!(summary.changes.len(), 1);
        assert_eq!(summary.changes[0].kind(), "modified");
    }

    /// The inference that makes the summary worth reading. "One removed, one
    /// added" loses the only fact a reviewer wants.
    #[test]
    fn a_rename_is_recognised_rather_than_reported_as_two_changes() {
        let summary = summary(
            "(defun old-name (x) (list x))\n",
            "(defun new-name (x) (list x))\n",
        );
        assert_eq!(summary.changes.len(), 1);
        assert_eq!(summary.changes[0].kind(), "renamed");
        let Change::Renamed { before, after } = &summary.changes[0] else {
            panic!("expected a rename");
        };
        assert_eq!(before.name.as_deref(), Some("old-name"));
        assert_eq!(after.name.as_deref(), Some("new-name"));
    }

    /// A rename is only inferred from an exact body match. A rename that also
    /// changed the body is two separate facts, and saying otherwise would be
    /// asserting an intent the evidence does not support.
    #[test]
    fn a_rename_that_also_changed_the_body_is_not_inferred() {
        let summary = summary(
            "(defun old-name (x) (list x))\n",
            "(defun new-name (x) (vector x))\n",
        );
        let kinds: Vec<&str> = summary.changes.iter().map(Change::kind).collect();
        assert_eq!(kinds, vec!["added", "removed"]);
    }

    /// Inserting one definition must not report every definition below it.
    #[test]
    fn inserting_above_reports_one_addition_and_the_moves() {
        let before = "(defun f (x) x)\n(defun g (y) y)\n";
        let after = "(defun new (z) z)\n(defun f (x) x)\n(defun g (y) y)\n";
        let summary = summary(before, after);

        assert_eq!(summary.count("added"), 1);
        assert_eq!(summary.count("removed"), 0);
        assert_eq!(summary.count("moved"), 2);
    }

    #[test]
    fn a_move_carries_both_paths() {
        let summary = summary(
            "(defun f (x) x)\n(defun g (y) y)\n",
            "(defun g (y) y)\n(defun f (x) x)\n",
        );
        let moves: Vec<&Change> = summary
            .changes
            .iter()
            .filter(|change| change.kind() == "moved")
            .collect();
        assert_eq!(moves.len(), 2);
        let Change::Moved { before, after } = moves[0] else {
            panic!("expected a move");
        };
        assert_ne!(before.path, after.path);
    }

    /// Two renames at once must pair up one-to-one rather than both claiming
    /// the same counterpart.
    #[test]
    fn two_renames_pair_off_without_reusing_a_counterpart() {
        let before = "(defun a (x) (list x))\n(defun b (y) (vector y))\n";
        let after = "(defun c (x) (list x))\n(defun d (y) (vector y))\n";
        let summary = summary(before, after);

        assert_eq!(summary.count("renamed"), 2);
        assert_eq!(summary.count("added"), 0);
        assert_eq!(summary.count("removed"), 0);
    }

    #[test]
    fn changes_are_reported_in_source_order() {
        let before = "(defun a (x) x)\n(defun b (y) y)\n(defun c (z) z)\n";
        let after = "(defun a (x) (list x))\n(defun b (y) y)\n(defun c (z) (list z))\n";
        let summary = summary(before, after);

        let lines: Vec<usize> = summary
            .changes
            .iter()
            .map(|change| change.anchor().line)
            .collect();
        assert!(lines.windows(2).all(|pair| pair[0] <= pair[1]), "{lines:?}");
    }

    /// An anonymous form still has to be nameable in a sentence, and two of
    /// them must not be paired just because neither has a name.
    #[test]
    fn a_form_with_no_name_is_labelled_by_its_path() {
        let summary = summary("(progn (list 1))\n", "(progn (list 1))\n(progn (list 2))\n");
        assert_eq!(summary.changes.len(), 1);
        assert!(
            summary.changes[0].anchor().label().contains("path"),
            "{}",
            summary.changes[0].anchor().label()
        );
    }

    /// The bug the exact name range exists to prevent: `d` occurs in `defun`
    /// long before it occurs as the name, so a first-occurrence replacement
    /// anonymises the wrong token and the rename is never seen.
    #[test]
    fn a_name_whose_letters_appear_in_its_own_operator_still_anonymises() {
        let summary = summary("(defun e (x) (list x))\n", "(defun d (x) (list x))\n");
        assert_eq!(summary.changes.len(), 1);
        assert_eq!(summary.changes[0].kind(), "renamed");
    }

    #[test]
    fn a_version_that_does_not_parse_yields_nothing_rather_than_a_wrong_answer() {
        assert!(summarise("(defun f (x", "(defun f (x) x)", CL).is_none());
        assert!(summarise("(defun f (x) x)", "(defun f (x", CL).is_none());
    }
}
