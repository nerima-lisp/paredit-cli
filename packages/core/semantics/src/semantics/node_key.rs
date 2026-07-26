//! Node identity for the semantic side tables.

use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView};

/// The identity of one node of a parsed document.
///
/// The semantic layers are read-only side tables hung beside the tree, so they
/// need a key for "this node". [`ExpressionView`] deliberately carries no
/// `NodeId` — it is a detached copy — which leaves the node's source extent as
/// the only available identity.
///
/// A bare [`ByteSpan`] is not quite enough: the virtual root and a
/// single-form document's only form can share one. Pairing the span with the
/// node's kind separates them, and excludes the root from ever colliding with
/// a real form. That every non-root node then has a unique `(span, kind)` is
/// the invariant the whole side-table design rests on, and it is pinned by a
/// property test in [`crate::domain::sexpr`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeKey {
    span: ByteSpan,
    kind: ExpressionKind,
}

impl NodeKey {
    /// The key for a view, or `None` for the virtual root — which is not a
    /// form and is deliberately outside every side table.
    pub fn of(view: &ExpressionView) -> Option<Self> {
        (view.kind != ExpressionKind::Root).then_some(Self {
            span: view.span,
            kind: view.kind,
        })
    }

    /// The key for an atom at `span`, for callers that hold a reference's span
    /// rather than its view (reference lists are collected as bare spans).
    pub const fn atom(span: ByteSpan) -> Self {
        Self {
            span,
            kind: ExpressionKind::Atom,
        }
    }

    pub const fn span(self) -> ByteSpan {
        self.span
    }

    pub const fn kind(self) -> ExpressionKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dialect::Dialect;
    use crate::domain::sexpr::{Path, SyntaxTree};

    fn root_form(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    #[test]
    fn the_virtual_root_has_no_key() {
        let tree = SyntaxTree::parse_with_dialect("(a)", Dialect::CommonLisp).expect("parse");
        assert_eq!(NodeKey::of(&tree.root_view()), None);
    }

    #[test]
    fn a_list_and_its_only_child_get_distinct_keys() {
        let form = root_form("(a)");
        let list = NodeKey::of(&form).expect("list key");
        let atom = NodeKey::of(&form.children[0]).expect("atom key");
        assert_ne!(list, atom);
    }

    #[test]
    fn an_atom_key_matches_the_key_built_from_its_view() {
        let form = root_form("(a)");
        let atom = &form.children[0];
        assert_eq!(NodeKey::of(atom), Some(NodeKey::atom(atom.span)));
    }
}
