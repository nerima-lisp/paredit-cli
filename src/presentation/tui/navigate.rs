//! Path arithmetic for `paredit tui`'s browsing loop.
//!
//! Deliberately has no notion of a terminal, a key, or a screen: every
//! function here is a pure `(tree, path) -> Option<path>` step, so the four
//! directions a person can move through a tree are testable exactly like any
//! other parser output, with an ordinary [`SyntaxTree::parse`] and no pty.
//!
//! [`ExpressionPath`] is already index-based (`ExpressionPath::child`,
//! `.parent()`), so every move here is "look at the current or a
//! neighboring view's child count, then build a new path from indexes" —
//! there is no separate tree-walking data structure to keep in sync with it.

use paredit_core_syntax::sexpr::{ExpressionPath, ExpressionView, SyntaxTree};

/// The view an (possibly empty) path names.
///
/// An empty path names the virtual root — the whole document, whose
/// children are the top-level forms — which [`SyntaxTree::select_path`]
/// itself refuses to select (there is no single form there to hand back),
/// so it is handled here instead of forcing every caller to special-case it.
pub(super) fn view_at(tree: &SyntaxTree, path: &ExpressionPath) -> Option<ExpressionView> {
    if path.to_raw_indexes().is_empty() {
        return Some(tree.root_view());
    }
    tree.select_path(path)
        .ok()
        .map(|selection| selection.view())
}

/// One level into the current node's first child, or `None` at a leaf.
pub(super) fn first_child(tree: &SyntaxTree, path: &ExpressionPath) -> Option<ExpressionPath> {
    let view = view_at(tree, path)?;
    (!view.children.is_empty()).then(|| path.child(0))
}

/// One level out to the parent, or `None` already at the root.
pub(super) fn parent(path: &ExpressionPath) -> Option<ExpressionPath> {
    path.parent()
}

/// The next sibling in source order, or `None` past the last one (including
/// at the root, which has no siblings of its own).
pub(super) fn next_sibling(tree: &SyntaxTree, path: &ExpressionPath) -> Option<ExpressionPath> {
    sibling(tree, path, 1)
}

/// The previous sibling in source order, or `None` before the first one.
pub(super) fn previous_sibling(tree: &SyntaxTree, path: &ExpressionPath) -> Option<ExpressionPath> {
    sibling(tree, path, -1)
}

fn sibling(tree: &SyntaxTree, path: &ExpressionPath, delta: isize) -> Option<ExpressionPath> {
    let indexes = path.to_raw_indexes();
    let (&own_index, prefix) = indexes.split_last()?;
    let parent_path = ExpressionPath::from_indexes(prefix.to_vec());
    let parent_view = view_at(tree, &parent_path)?;
    let candidate = own_index.checked_add_signed(delta)?;
    (candidate < parent_view.children.len()).then(|| parent_path.child(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> SyntaxTree {
        // Three top-level forms; the second has two children of its own, so
        // both sibling movement and descent/ascent have something to cross.
        SyntaxTree::parse("(a)\n(b 1 2)\n(c)\n").expect("fixture parses")
    }

    fn root() -> ExpressionPath {
        ExpressionPath::from_indexes(Vec::new())
    }

    #[test]
    fn the_root_view_lists_every_top_level_form_as_a_child() {
        let tree = tree();
        let view = view_at(&tree, &root()).expect("root always resolves");
        assert_eq!(view.children.len(), 3);
    }

    #[test]
    fn first_child_descends_into_the_first_top_level_form() {
        let tree = tree();
        let path = first_child(&tree, &root()).expect("root has children");
        assert_eq!(path.to_raw_indexes(), vec![0]);
    }

    #[test]
    fn first_child_at_a_leaf_atom_has_nowhere_to_descend() {
        let tree = tree();
        // `(a)` is a list with one child of its own — the atom `a` — so the
        // true leaf here is that atom, one level past the list itself.
        let atom_a = ExpressionPath::from_indexes(vec![0, 0]);
        assert!(first_child(&tree, &atom_a).is_none());
    }

    #[test]
    fn next_and_previous_sibling_cross_the_top_level_forms_in_order() {
        let tree = tree();
        let a = ExpressionPath::from_indexes(vec![0]);
        let b = next_sibling(&tree, &a).expect("a form follows a");
        assert_eq!(b.to_raw_indexes(), vec![1]);
        let back_to_a = previous_sibling(&tree, &b).expect("a form precedes b");
        assert_eq!(back_to_a.to_raw_indexes(), vec![0]);
    }

    #[test]
    fn next_sibling_past_the_last_top_level_form_is_none() {
        let tree = tree();
        let c = ExpressionPath::from_indexes(vec![2]);
        assert!(next_sibling(&tree, &c).is_none());
    }

    #[test]
    fn previous_sibling_before_the_first_top_level_form_is_none() {
        let tree = tree();
        let a = ExpressionPath::from_indexes(vec![0]);
        assert!(previous_sibling(&tree, &a).is_none());
    }

    #[test]
    fn the_root_has_no_siblings_of_its_own() {
        let tree = tree();
        assert!(next_sibling(&tree, &root()).is_none());
        assert!(previous_sibling(&tree, &root()).is_none());
    }

    #[test]
    fn parent_of_a_top_level_form_is_the_root() {
        let a = ExpressionPath::from_indexes(vec![0]);
        assert_eq!(parent(&a), Some(root()));
    }

    #[test]
    fn parent_of_the_root_is_none() {
        assert!(parent(&root()).is_none());
    }

    #[test]
    fn descending_then_moving_between_a_nested_forms_own_children() {
        let tree = tree();
        let b = ExpressionPath::from_indexes(vec![1]);
        // `(b 1 2)` has three children of its own: the operator atom `b` and
        // its two argument atoms.
        let operator = first_child(&tree, &b).expect("b has children");
        assert_eq!(operator.to_raw_indexes(), vec![1, 0]);
        let first_operand =
            next_sibling(&tree, &operator).expect("an operand follows the operator");
        assert_eq!(first_operand.to_raw_indexes(), vec![1, 1]);
        let second_operand =
            next_sibling(&tree, &first_operand).expect("a second operand follows the first");
        assert_eq!(second_operand.to_raw_indexes(), vec![1, 2]);
        assert!(next_sibling(&tree, &second_operand).is_none());
        assert_eq!(parent(&second_operand), Some(b));
    }
}
