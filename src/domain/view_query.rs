//! Small shared queries and traversals over parsed [`ExpressionView`]s.
//!
//! Every report that walks a whole form tree looking for a particular head
//! (`case`, `cond`, `if`, `setf`, …) needs the same handful of one-liners —
//! "is this a parenthesized list", "what is its head symbol", "the atom text
//! of child N" — plus the same pre-order recursion. Keeping one copy here
//! means a report body is just its own analysis, not re-derived boilerplate.

use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

/// The atom's text, or `None` for a non-atom (or a text-less atom).
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The atom text of the child at `index`, if that child exists and is an atom.
pub(crate) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

/// Whether `view` is a `(...)` list (as opposed to an atom, a `[...]`
/// bracket, or a `{...}` brace form).
pub(crate) fn is_paren_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren)
}

/// The head symbol of a `(...)` list — its first child's atom text — or
/// `None` if `view` is not a paren list or its head is not a bare symbol.
pub(crate) fn list_head(view: &ExpressionView) -> Option<&str> {
    is_paren_list(view).then(|| atom_child(view, 0)).flatten()
}

/// Calls `visit` on `root` and every descendant view, in pre-order (a view
/// before its children). The single place the whole-tree recursion that
/// body-form lints share is written.
pub(crate) fn for_each_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    fn recurse(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
        visit(view);
        for child in &view.children {
            recurse(child, visit);
        }
    }
    recurse(root, &mut visit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sexpr::{Path, SyntaxTree};

    fn root(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse(input).expect("parse input");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    #[test]
    fn list_head_reads_the_first_symbol() {
        assert_eq!(list_head(&root("(defun f ())")), Some("defun"));
    }

    #[test]
    fn list_head_is_none_for_an_atom() {
        assert_eq!(list_head(&root("x")), None);
    }

    #[test]
    fn atom_child_reads_a_positional_symbol() {
        assert_eq!(atom_child(&root("(defun f ())"), 1), Some("f"));
    }

    #[test]
    fn for_each_subview_visits_root_and_all_descendants_preorder() {
        let mut heads = Vec::new();
        for_each_subview(&root("(a (b) (c (d)))"), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        assert_eq!(heads, vec!["a", "b", "c", "d"]);
    }
}
