//! Carrying a structural difference from one pair of files onto a third.
//!
//! The use this exists for: a fix is made in one file, and the same shape
//! appears in four others. A text patch cannot travel, because the surrounding
//! lines differ. A structural change can, because what it names is a *form* —
//! and a form is findable by its shape wherever it was written.
//!
//! Each change carries an *anchor* — a form to look for, chosen by the diff
//! (see `Portable`) — which is hashed and looked up in the target's subtree
//! index. Three outcomes, all reported rather than resolved silently:
//!
//! - **one match** — the change applies there.
//! - **no match** — the target does not have the form the change edits. Not an
//!   error; usually it means this file was already correct, or never had the
//!   problem.
//! - **several matches** — ambiguous. Applying to all of them is a defensible
//!   choice and it is `--all`'s, not the default's, because "the same form
//!   appears twice" and "the same *bug* appears twice" are different claims and
//!   only the caller can tell them apart.
//!
//! A top-level insertion has no anchor on either side — it names no form that
//! exists in the target — and is reported as unportable rather than placed by
//! guesswork.

use std::collections::BTreeSet;

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::structural_diff::usecase::{Change, ChangeKind, index_subtrees, shape_hash};

/// What became of one change when carried to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Matched exactly once, and is in the edit plan.
    Applied,
    /// The target has nothing shaped like the change's "before" side.
    NotFound,
    /// Several forms in the target match. Applied only with `--all`.
    Ambiguous,
    /// The change has no "before" side to anchor on.
    Unportable,
}

impl Outcome {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::NotFound => "not-found",
            Self::Ambiguous => "ambiguous",
            Self::Unportable => "unportable",
        }
    }

    /// Whether this outcome contributes edits to the plan.
    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Self::Applied)
    }
}

/// One change, resolved against the target.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub kind: ChangeKind,
    pub outcome: Outcome,
    /// The change's path in the source pair, so a caller can trace a resolution
    /// back to the diff that produced it.
    pub source_path: String,
    pub head: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    /// The form searched for in the target. `None` when nothing anchored it.
    pub anchor: Option<String>,
    /// Whether the anchor is the enclosing form rather than the change's own
    /// before side. Reported because a caller should be able to see that a
    /// change was matched by more context than it names.
    pub widened: bool,
    /// Where in the target the change lands. Empty unless it matched.
    pub targets: Vec<ByteSpan>,
}

/// The full plan: every change's fate, and the byte edits to make.
#[derive(Debug, Clone)]
pub struct PatchPlan {
    pub resolutions: Vec<Resolution>,
    pub edits: Vec<(ByteSpan, String)>,
}

impl PatchPlan {
    #[must_use]
    pub fn applied_count(&self) -> usize {
        self.resolutions
            .iter()
            .filter(|resolution| resolution.outcome.writes())
            .count()
    }

    #[must_use]
    pub fn count_with(&self, outcome: Outcome) -> usize {
        self.resolutions
            .iter()
            .filter(|resolution| resolution.outcome == outcome)
            .count()
    }
}

/// Resolves every change against `target`, producing the plan.
///
/// `apply_ambiguous` is `--all`: it turns an ambiguous match into edits at every
/// site instead of none.
#[must_use]
pub fn plan_patch(changes: &[Change], target: &ExpressionView, apply_ambiguous: bool) -> PatchPlan {
    let index = index_subtrees(target);
    let mut resolutions = Vec::with_capacity(changes.len());
    let mut edits: Vec<(ByteSpan, String)> = Vec::new();

    // Two changes inside one form widen to the same anchor, and applying that
    // anchor twice would double-count what is one rewrite. The replacement
    // already carries both edits — it is the whole new form — so the second is
    // redundant rather than lost.
    let mut planned: BTreeSet<String> = BTreeSet::new();

    for change in changes {
        let head = change.head();
        let before_text = change.before.as_ref().map(|side| side.text.clone());
        let after_text = change.after.as_ref().map(|side| side.text.clone());

        let Some(portable) = &change.portable else {
            resolutions.push(Resolution {
                kind: change.kind,
                outcome: Outcome::Unportable,
                source_path: change.path.clone(),
                head,
                before: before_text,
                after: after_text,
                anchor: None,
                widened: false,
                targets: Vec::new(),
            });
            continue;
        };

        let matches = hash_of_source(&portable.anchor)
            .and_then(|hash| index.get(&hash).cloned())
            .unwrap_or_default();

        let first_of_its_anchor = planned.insert(portable.anchor.clone());
        let outcome = match matches.len() {
            0 => Outcome::NotFound,
            1 => Outcome::Applied,
            _ if apply_ambiguous => Outcome::Applied,
            _ => Outcome::Ambiguous,
        };

        if outcome.writes() && first_of_its_anchor {
            edits.extend(
                matches
                    .iter()
                    .map(|span| (*span, portable.replacement.clone())),
            );
        }

        resolutions.push(Resolution {
            kind: change.kind,
            outcome,
            source_path: change.path.clone(),
            head,
            before: before_text,
            after: after_text,
            anchor: Some(portable.anchor.clone()),
            widened: portable.widened,
            targets: if outcome.writes() || outcome == Outcome::Ambiguous {
                matches
            } else {
                Vec::new()
            },
        });
    }

    // Overlapping edits cannot both be applied, and an edit nested inside
    // another is exactly the case a naive reverse-order application would
    // corrupt: the outer replacement rewrites the bytes the inner one was
    // measured against. Dropping the inner one keeps the wider change, which
    // is the one that carries more of the source difference.
    edits.sort_by_key(|(span, _)| (span.start().get(), std::cmp::Reverse(span.end().get())));
    let mut kept: Vec<(ByteSpan, String)> = Vec::new();
    for (span, text) in edits {
        let overlaps = kept
            .last()
            .is_some_and(|(previous, _)| span.start().get() < previous.end().get());
        if !overlaps {
            kept.push((span, text));
        }
    }

    PatchPlan {
        resolutions,
        edits: kept,
    }
}

/// The shape hash of a form given as text.
///
/// Reparsing rather than carrying the hash from the diff, because the diff and
/// the patch can run in separate invocations — the whole point of `--plan`
/// being a document — and a hash is only as portable as the parse behind it.
fn hash_of_source(text: &str) -> Option<[u8; 32]> {
    let tree = paredit_core_syntax::sexpr::SyntaxTree::parse(text).ok()?;
    let root = tree.root_view();
    // One form was hashed on the diff side, so one form must be hashed here;
    // a text that parses as two is not the same thing at all.
    (root.children.len() == 1).then(|| shape_hash(&root.children[0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_diff::usecase::diff_documents;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn changes(old: &str, new: &str) -> Vec<Change> {
        let old_tree = SyntaxTree::parse(old).expect("old parses");
        let new_tree = SyntaxTree::parse(new).expect("new parses");
        diff_documents(&old_tree.root_view(), old, &new_tree.root_view(), new)
    }

    fn plan(old: &str, new: &str, target: &str, all: bool) -> (PatchPlan, String) {
        let tree = SyntaxTree::parse(target).expect("target parses");
        let plan = plan_patch(&changes(old, new), &tree.root_view(), all);
        let patched = paredit_core_cli::shared::apply_byte_span_edits(target, plan.edits.clone())
            .expect("edits apply");
        (plan, patched)
    }

    #[test]
    fn a_change_lands_wherever_the_target_wrote_the_same_form() {
        let (plan, patched) = plan(
            "(defun a () (car (reverse xs)))",
            "(defun a () (first (reverse xs)))",
            "(defun elsewhere (xs)\n  (car (reverse xs)))\n",
            false,
        );
        assert_eq!(plan.applied_count(), 1);
        assert!(patched.contains("(first (reverse xs))"), "{patched}");
    }

    /// Formatting must not decide whether a change ports. The target below
    /// writes the same form across three lines.
    #[test]
    fn matching_ignores_the_targets_formatting() {
        let (plan, patched) = plan(
            "(f (g 1 2))",
            "(f (h 1 2))",
            "(other\n  (g 1\n     2))\n",
            false,
        );
        assert_eq!(plan.applied_count(), 1, "{:?}", plan.resolutions);
        assert!(patched.contains("(h 1 2)"), "{patched}");
    }

    #[test]
    fn a_target_without_the_form_is_reported_rather_than_failing() {
        let (plan, patched) = plan("(f (g 1))", "(f (h 1))", "(unrelated 1)\n", false);
        assert_eq!(plan.count_with(Outcome::NotFound), 1);
        assert_eq!(patched, "(unrelated 1)\n");
    }

    /// Two sites, and only the caller knows whether both have the bug.
    #[test]
    fn two_matching_sites_are_ambiguous_until_all_is_given() {
        let target = "(one (g 1))\n(two (g 1))\n";
        let (cautious, unchanged) = plan("(f (g 1))", "(f (h 1))", target, false);
        assert_eq!(cautious.count_with(Outcome::Ambiguous), 1);
        assert_eq!(unchanged, target, "nothing written while ambiguous");

        let (forced, patched) = plan("(f (g 1))", "(f (h 1))", target, true);
        assert_eq!(forced.applied_count(), 1);
        assert_eq!(patched.matches("(h 1)").count(), 2, "{patched}");
    }

    /// An insertion inside a form has no before side of its own, but the form
    /// it went into is a perfectly good anchor — so an added argument ports.
    #[test]
    fn an_insertion_inside_a_form_ports_by_widening_to_that_form() {
        let (plan, patched) = plan("(f a)", "(f a b)", "(wrap\n  (f a))\n", false);
        assert_eq!(plan.applied_count(), 1);
        assert!(plan.resolutions[0].widened, "{:?}", plan.resolutions[0]);
        assert!(patched.contains("(f a b)"), "{patched}");
    }

    /// A *top-level* insertion is the case with nothing to widen to: it names
    /// no form that exists in the target, on either side.
    #[test]
    fn a_top_level_insertion_is_reported_unportable_rather_than_placed_by_guess() {
        let (plan, patched) = plan(
            "(defun a () 1)\n",
            "(defun a () 1)\n(defun b () 2)\n",
            "(defun a () 1)\n",
            false,
        );
        assert_eq!(plan.count_with(Outcome::Unportable), 1);
        assert_eq!(patched, "(defun a () 1)\n");
    }

    /// A change to a bare atom is a change to a token, and a token is not a
    /// place. Anchoring on `car` alone would rewrite every `car` in the file.
    #[test]
    fn an_atom_change_anchors_on_its_enclosing_form() {
        let (plan, patched) = plan(
            "(defun r (xs) (car (reverse xs)))",
            "(defun r (xs) (first (reverse xs)))",
            "(defun elsewhere (xs)\n  (car xs)\n  (car (reverse xs)))\n",
            false,
        );
        assert_eq!(plan.applied_count(), 1);
        assert_eq!(
            plan.resolutions[0].anchor.as_deref(),
            Some("(car (reverse xs))"),
            "the anchor widens past the bare `car`",
        );
        // The unrelated `(car xs)` above is untouched, which the bare-atom
        // anchor would not have managed.
        assert!(patched.contains("(car xs)"), "{patched}");
        assert!(patched.contains("(first (reverse xs))"), "{patched}");
    }

    /// Two edits inside one form widen to the same anchor. The replacement is
    /// the whole new form and already carries both, so the second must not
    /// plan a duplicate rewrite.
    #[test]
    fn two_edits_in_one_form_plan_a_single_rewrite() {
        let (plan, patched) = plan("(f a b)", "(f A B)", "(outer (f a b))\n", false);
        assert_eq!(plan.edits.len(), 1, "{:?}", plan.edits);
        assert!(patched.contains("(f A B)"), "{patched}");
    }

    #[test]
    fn a_deletion_removes_the_matching_form() {
        let (plan, patched) = plan(
            "(progn (old-call) (keep))",
            "(progn (keep))",
            "(defun elsewhere ()\n  (old-call)\n  (keep))\n",
            false,
        );
        assert_eq!(plan.applied_count(), 1);
        assert!(!patched.contains("old-call"), "{patched}");
        assert!(patched.contains("(keep)"), "{patched}");
    }

    /// Two edits where one sits inside the other cannot both be applied; the
    /// outer replacement rewrites the bytes the inner one was measured against.
    #[test]
    fn nested_edits_do_not_corrupt_each_other() {
        let (plan, patched) = plan("(a (b (c 1)))", "(a (B (C 2)))", "(a (b (c 1)))\n", false);
        assert!(
            plan.edits
                .windows(2)
                .all(|pair| { pair[0].0.end().get() <= pair[1].0.start().get() }),
            "edits must not overlap: {:?}",
            plan.edits
        );
        assert!(
            SyntaxTree::parse(&patched).is_ok(),
            "the patched source must still parse: {patched}"
        );
    }
}
