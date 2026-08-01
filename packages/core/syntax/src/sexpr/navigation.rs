//! The two read-only questions an editing agent has to answer before it can
//! name a target: *where does this move go*, and *what kind of text is at this
//! offset*.
//!
//! Both exist because every mutating command in this tool addresses a form by
//! `--path` or `--at`, and building those by hand is the expensive part of
//! driving it. [`Selection::navigate`] turns "one to the right" into a path;
//! [`SyntaxTree::context_at`] answers whether an offset is code at all, so a
//! cursor edit can be refused before it corrupts a string or a comment rather
//! than after.

use super::error::{SexprResult, StructureError};
use super::tree::{Node, NodeKind, Selection, SyntaxTree};
use super::types::{ByteOffset, ByteSpan, Delimiter, ExpressionPath, NodeId};

/// One structural move, matching the four directions `paredit.el` binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The next sibling in the enclosing list.
    Forward,
    /// The previous sibling in the enclosing list.
    Backward,
    /// The enclosing expression.
    Up,
    /// The first child expression.
    Down,
}

impl Direction {
    /// The direction's name, as the CLI spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

impl<'a> Selection<'a> {
    /// Moves one step in `direction` and returns the selection that lands there.
    ///
    /// Strictly one step: `forward` is the next *sibling*, never an ancestor's
    /// sibling. Emacs moves point out of a list when it runs off the end, which
    /// is right for a cursor and wrong for an address — an agent composing
    /// `navigate` calls needs each one to fail visibly at the boundary rather
    /// than silently change depth.
    pub fn navigate(self, direction: Direction) -> SexprResult<Self> {
        let node_id = match direction {
            Direction::Forward => {
                sibling(self.tree, self.node_id, 1).ok_or(StructureError::NoNextSibling)?
            }
            Direction::Backward => {
                sibling(self.tree, self.node_id, -1).ok_or(StructureError::NoPreviousSibling)?
            }
            Direction::Up => {
                let parent = self
                    .node()
                    .parent
                    .ok_or(StructureError::NoEnclosingExpression)?;
                if self.tree.node(parent).kind == NodeKind::Root {
                    return Err(StructureError::NoEnclosingExpression.into());
                }
                parent
            }
            Direction::Down => *self
                .node()
                .children
                .first()
                .ok_or(StructureError::NoChildExpression)?,
        };
        Ok(Self {
            tree: self.tree,
            node_id,
        })
    }
}

fn sibling(tree: &SyntaxTree, node_id: NodeId, offset: isize) -> Option<NodeId> {
    let parent = tree.node(node_id).parent?;
    let siblings = &tree.node(parent).children;
    let position = siblings.iter().position(|id| *id == node_id)?;
    let target = position.checked_add_signed(offset)?;
    siblings.get(target).copied()
}

/// What kind of source text an offset falls in.
///
/// Ordered from "an edit here is plain text" to "an edit here changes what the
/// reader sees", which is the order the CLI reports them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    /// Between forms: inside a list but not inside any of its children, or
    /// between two top-level forms.
    Whitespace,
    /// Inside an atom's own symbol text.
    Code,
    /// Inside a string literal, delimiters included.
    String,
    /// Inside a comment, in any of the four spellings the parser records.
    Comment,
    /// On one of the characters of a reader prefix (`'`, `` ` ``, `#'`, `,`, …).
    ReaderPrefix,
    /// On a list's opening or closing delimiter.
    Delimiter,
}

impl ContextKind {
    /// The kind's name, as the CLI prints it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Whitespace => "whitespace",
            Self::Code => "code",
            Self::String => "string",
            Self::Comment => "comment",
            Self::ReaderPrefix => "reader-prefix",
            Self::Delimiter => "delimiter",
        }
    }

    /// Whether inserting or deleting one character here leaves the document's
    /// structure alone.
    ///
    /// `Code` and `Whitespace` are the two places a plain text edit cannot
    /// change the tree's shape. Everything else either carries structure
    /// (`Delimiter`, `ReaderPrefix`) or hides it (`String`, `Comment`), and the
    /// cursor edits refuse accordingly.
    #[must_use]
    pub const fn is_structurally_inert(self) -> bool {
        matches!(self, Self::Whitespace | Self::Code)
    }
}

/// What [`SyntaxTree::context_at`] found at one byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContext {
    pub offset: usize,
    pub kind: ContextKind,
    /// The innermost expression containing the offset, when there is one. An
    /// offset in top-level whitespace or a top-level comment has none.
    pub path: Option<ExpressionPath>,
    pub span: Option<ByteSpan>,
    /// The innermost enclosing *list*, which is what a structural edit at this
    /// offset would act inside.
    pub enclosing_list_path: Option<ExpressionPath>,
    pub enclosing_list_span: Option<ByteSpan>,
    /// The enclosing list's head symbol, when it has one.
    pub enclosing_head: Option<String>,
    /// How many lists enclose the offset.
    pub depth: usize,
    /// The open delimiters enclosing the offset, outermost first. Closing this
    /// stack is exactly what balancing the document at this point requires.
    pub delimiter_stack: Vec<char>,
}

impl SyntaxTree {
    /// Reports what kind of text sits at `offset`, and what encloses it.
    ///
    /// The offset may equal the document length: that is the position after the
    /// last byte, where an append happens, and refusing it would make the
    /// end-of-buffer case unaddressable.
    pub fn context_at(&self, offset: usize) -> SexprResult<SourceContext> {
        let source = self.source();
        if offset > source.len() {
            return Err(StructureError::OffsetOutsideDocument {
                offset,
                length: source.len(),
            }
            .into());
        }

        let innermost = self.innermost_node_at(offset);
        let enclosing_list = innermost.and_then(|node_id| self.enclosing_list_of(node_id, offset));

        let kind = self.classify(offset, innermost);
        let (delimiter_stack, depth) = enclosing_list
            .map(|node_id| self.delimiter_stack(node_id))
            .unwrap_or_default();

        Ok(SourceContext {
            offset,
            kind,
            path: innermost.map(|node_id| self.selection(node_id).path()),
            span: innermost.map(|node_id| self.node(node_id).span),
            enclosing_list_path: enclosing_list.map(|node_id| self.selection(node_id).path()),
            enclosing_list_span: enclosing_list.map(|node_id| self.node(node_id).span),
            enclosing_head: enclosing_list
                .and_then(|node_id| self.selection(node_id).head())
                .map(str::to_owned),
            depth,
            delimiter_stack,
        })
    }

    fn classify(&self, offset: usize, innermost: Option<NodeId>) -> ContextKind {
        // Comments are not nodes, so they are checked first: an offset inside a
        // comment that sits inside a list resolves to the list otherwise.
        if self
            .comments
            .iter()
            .any(|comment| comment.span.contains(ByteOffset::new(offset)))
        {
            return ContextKind::Comment;
        }

        let Some(node_id) = innermost else {
            return ContextKind::Whitespace;
        };
        let node = self.node(node_id);
        if node
            .reader_prefix_spans()
            .iter()
            .any(|span| span.contains(ByteOffset::new(offset)))
        {
            return ContextKind::ReaderPrefix;
        }

        match node.kind {
            NodeKind::Root => ContextKind::Whitespace,
            NodeKind::Atom => {
                if is_string_atom(node, self.source()) {
                    ContextKind::String
                } else {
                    ContextKind::Code
                }
            }
            NodeKind::List => {
                let at_delimiter = node.open.is_some_and(|open| open.get() == offset)
                    || node.close.is_some_and(|close| close.get() == offset);
                if at_delimiter {
                    ContextKind::Delimiter
                } else {
                    ContextKind::Whitespace
                }
            }
        }
    }

    /// The smallest non-root node whose span contains `offset`, if any.
    pub(in crate::sexpr) fn innermost_node_at(&self, offset: usize) -> Option<NodeId> {
        let offset = ByteOffset::new(offset);
        let mut best: Option<NodeId> = None;
        for index in 1..self.nodes.len() {
            let node_id = NodeId::new(index);
            let node = self.node(node_id);
            if !node.span.contains(offset) {
                continue;
            }
            match best {
                Some(current) if self.node(current).span.len() <= node.span.len() => {}
                _ => best = Some(node_id),
            }
        }
        best
    }

    /// The innermost list that encloses `offset`, starting the walk at `node_id`.
    fn enclosing_list_of(&self, node_id: NodeId, offset: usize) -> Option<NodeId> {
        let node = self.node(node_id);
        // An offset on a list's own delimiter is enclosed by that list's
        // parent, not by the list itself: deleting the delimiter acts on the
        // parent's children.
        let start_at_parent = node.kind == NodeKind::List
            && (node.open.is_some_and(|open| open.get() == offset)
                || node.close.is_some_and(|close| close.get() == offset));
        let mut current = if start_at_parent {
            node.parent?
        } else {
            node_id
        };
        loop {
            let node = self.node(current);
            if node.kind == NodeKind::List {
                return Some(current);
            }
            current = node.parent?;
        }
    }

    /// The open delimiters enclosing `node_id`, outermost first, and the depth.
    fn delimiter_stack(&self, node_id: NodeId) -> (Vec<char>, usize) {
        let mut stack = Vec::new();
        let mut current = Some(node_id);
        while let Some(id) = current {
            let node = self.node(id);
            if node.kind == NodeKind::List {
                stack.push(node.delimiter.map_or('(', Delimiter::open));
            }
            current = node.parent;
        }
        stack.reverse();
        let depth = stack.len();
        (stack, depth)
    }

    pub(in crate::sexpr) const fn selection(&self, node_id: NodeId) -> Selection<'_> {
        Selection {
            tree: self,
            node_id,
        }
    }
}

/// Whether an atom's own text — past any reader prefix — is a string literal.
pub(in crate::sexpr) fn is_string_atom(node: &Node, source: &str) -> bool {
    node.kind == NodeKind::Atom
        && node
            .span
            .slice(source)
            .get(node.symbol_offset as usize..)
            .is_some_and(|text| text.starts_with('"'))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sexpr::ExpressionPath;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse(source).unwrap()
    }

    fn path_of(source: &str, path: &str, direction: Direction) -> String {
        let tree = tree(source);
        let selection = tree
            .select_path(&path.parse::<ExpressionPath>().unwrap())
            .unwrap();
        selection.navigate(direction).unwrap().path().to_string()
    }

    #[test]
    fn forward_and_backward_walk_siblings() {
        assert_eq!(path_of("(a b c)", "0.0", Direction::Forward), "0.1");
        assert_eq!(path_of("(a b c)", "0.2", Direction::Backward), "0.1");
    }

    #[test]
    fn up_and_down_change_depth() {
        assert_eq!(path_of("(a (b c))", "0.1.0", Direction::Up), "0.1");
        assert_eq!(path_of("(a (b c))", "0.1", Direction::Down), "0.1.0");
    }

    #[test]
    fn forward_refuses_rather_than_leaving_the_list() {
        let tree = tree("(a b) (c)");
        let selection = tree
            .select_path(&"0.1".parse::<ExpressionPath>().unwrap())
            .unwrap();
        assert!(selection.navigate(Direction::Forward).is_err());
    }

    #[test]
    fn up_from_a_top_level_form_refuses() {
        let tree = tree("(a)");
        let selection = tree
            .select_path(&"0".parse::<ExpressionPath>().unwrap())
            .unwrap();
        assert!(selection.navigate(Direction::Up).is_err());
    }

    #[test]
    fn path_round_trips_through_select_path() {
        let tree = tree("(defun f (x)\n  (list x))");
        let selection = tree.select_at(20).unwrap();
        let path = selection.path();
        assert_eq!(tree.select_path(&path).unwrap().span(), selection.span());
    }

    #[test]
    fn context_distinguishes_string_comment_and_code() {
        let source = "(foo \"bar\" ; note\n  baz)";
        let tree = tree(source);
        let kind_at = |offset: usize| tree.context_at(offset).unwrap().kind;

        assert_eq!(kind_at(source.find("foo").unwrap()), ContextKind::Code);
        assert_eq!(kind_at(source.find("bar").unwrap()), ContextKind::String);
        assert_eq!(kind_at(source.find("note").unwrap()), ContextKind::Comment);
        assert_eq!(kind_at(0), ContextKind::Delimiter);
        assert_eq!(
            kind_at(source.find("  baz").unwrap()),
            ContextKind::Whitespace
        );
    }

    #[test]
    fn context_reports_the_reader_prefix_separately_from_its_form() {
        let source = "(a #'f)";
        let tree = tree(source);
        assert_eq!(
            tree.context_at(source.find("#'").unwrap()).unwrap().kind,
            ContextKind::ReaderPrefix
        );
        assert_eq!(
            tree.context_at(source.find("f)").unwrap()).unwrap().kind,
            ContextKind::Code
        );
    }

    #[test]
    fn context_reports_the_enclosing_list_and_its_delimiter_stack() {
        let source = "(let ((x 1))\n  (+ x 1))";
        let tree = tree(source);
        let context = tree.context_at(source.find("+ x").unwrap()).unwrap();
        assert_eq!(context.enclosing_head.as_deref(), Some("+"));
        assert_eq!(context.depth, 2);
        assert_eq!(context.delimiter_stack, vec!['(', '(']);
    }

    #[test]
    fn context_accepts_the_offset_one_past_the_last_byte() {
        let source = "(a)";
        let context = tree(source).context_at(source.len()).unwrap();
        assert_eq!(context.kind, ContextKind::Whitespace);
        assert_eq!(context.depth, 0);
    }

    #[test]
    fn context_refuses_an_offset_past_the_document() {
        assert!(tree("(a)").context_at(9).is_err());
    }
}
