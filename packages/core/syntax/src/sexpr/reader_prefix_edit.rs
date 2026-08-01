//! Adding and removing the reader sugar in front of a form.
//!
//! `paredit.el` has no command for this because in Emacs a quote is one
//! self-inserting character. Through a path-addressed CLI it is not: `'x` and
//! `x` are the same node with different spans, so quoting a form by hand means
//! computing where its prefixes end — which is precisely the byte arithmetic
//! this tool exists to take over.
//!
//! Both directions work on the *outermost* prefix. `#'` and `'` compose in
//! source order, so inserting at the node's start and deleting from the node's
//! start are inverses, and `unwrap-prefix` twice undoes `wrap --prefix` twice.

use super::edit::Edit;
use super::error::{SexprResult, StructureError};
use super::tree::{Node, NodeKind, ReaderPrefix, Selection, SyntaxTree};
use super::types::{ByteOffset, ByteSpan};

impl Edit {
    /// Prefixes the selected expression with `prefix`, outside any prefix it
    /// already carries.
    ///
    /// `x` with `Quote` becomes `'x`; `#'f` with `Quote` becomes `'#'f`, which
    /// is the nesting order the reader gives the two prefixes when they are
    /// typed in that sequence.
    pub fn wrap_prefix(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        prefix: ReaderPrefix,
    ) -> SexprResult<String> {
        super::edit::validate_edit_context(input, tree, selection)?;
        let start = selection.span().start().get();
        let mut output = String::with_capacity(input.len() + 3);
        output.push_str(&input[..start]);
        output.push_str(prefix.as_source());
        output.push_str(&input[start..]);
        Ok(output)
    }

    /// Removes the selected expression's reader prefixes, outermost first.
    ///
    /// With `all`, every prefix goes; otherwise only the outermost one. Any
    /// trivia between the removed prefix and what follows goes with it, because
    /// `#' foo` is legal syntax and leaving the space behind would strand it in
    /// front of the form.
    pub fn unwrap_prefix(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        all: bool,
    ) -> SexprResult<String> {
        super::edit::validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        let spans = node.reader_prefix_spans();
        let first = *spans
            .first()
            .ok_or(StructureError::NoReaderPrefixToUnwrap)?;

        // The removal runs to wherever the *next* thing starts, so the trivia a
        // prefix is allowed to be followed by is removed with it.
        let end = if all || spans.len() == 1 {
            content_start(node)
        } else {
            spans[1].start()
        };

        let mut output = String::with_capacity(input.len());
        output.push_str(&input[..first.start().get()]);
        output.push_str(&input[end.get()..]);
        Ok(output)
    }
}

/// Where a node's own text begins, past every reader prefix and the trivia the
/// parser allowed between them.
pub(in crate::sexpr) fn content_start(node: &Node) -> ByteOffset {
    match node.kind {
        NodeKind::Atom => ByteOffset::new(node.span.start().get() + node.symbol_offset as usize),
        NodeKind::List => node.open.unwrap_or_else(|| node.span.start()),
        NodeKind::Root => node.span.start(),
    }
}

/// The span a node's own text occupies, excluding its reader prefixes.
pub(in crate::sexpr) fn content_span(node: &Node) -> ByteSpan {
    ByteSpan::new(content_start(node), node.span.end())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sexpr::ExpressionPath;

    fn apply(
        source: &str,
        path: &str,
        edit: impl Fn(&str, &SyntaxTree, Selection<'_>) -> SexprResult<String>,
    ) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        let selection = tree
            .select_path(&path.parse::<ExpressionPath>().unwrap())
            .unwrap();
        edit(source, &tree, selection)
    }

    #[test]
    fn wrap_prefix_quotes_an_atom() {
        let output = apply("(list x)", "0.1", |input, tree, selection| {
            Edit::wrap_prefix(input, tree, selection, ReaderPrefix::Quote)
        })
        .unwrap();
        assert_eq!(output, "(list 'x)");
    }

    #[test]
    fn wrap_prefix_nests_outside_an_existing_prefix() {
        let output = apply("(list #'f)", "0.1", |input, tree, selection| {
            Edit::wrap_prefix(input, tree, selection, ReaderPrefix::Quote)
        })
        .unwrap();
        assert_eq!(output, "(list '#'f)");
    }

    #[test]
    fn wrap_prefix_applies_to_a_list_as_well() {
        let output = apply("(list (a b))", "0.1", |input, tree, selection| {
            Edit::wrap_prefix(input, tree, selection, ReaderPrefix::Quasiquote)
        })
        .unwrap();
        assert_eq!(output, "(list `(a b))");
    }

    #[test]
    fn unwrap_prefix_removes_only_the_outermost() {
        let output = apply("(list '#'f)", "0.1", |input, tree, selection| {
            Edit::unwrap_prefix(input, tree, selection, false)
        })
        .unwrap();
        assert_eq!(output, "(list #'f)");
    }

    #[test]
    fn unwrap_prefix_all_removes_every_prefix() {
        let output = apply("(list '#'f)", "0.1", |input, tree, selection| {
            Edit::unwrap_prefix(input, tree, selection, true)
        })
        .unwrap();
        assert_eq!(output, "(list f)");
    }

    #[test]
    fn unwrap_prefix_takes_the_trivia_a_prefix_is_allowed_to_carry() {
        let output = apply("(list #' f)", "0.1", |input, tree, selection| {
            Edit::unwrap_prefix(input, tree, selection, false)
        })
        .unwrap();
        assert_eq!(output, "(list f)");
    }

    #[test]
    fn unwrap_prefix_refuses_a_form_with_no_prefix() {
        let error = apply("(list x)", "0.1", |input, tree, selection| {
            Edit::unwrap_prefix(input, tree, selection, false)
        })
        .unwrap_err();
        assert!(error.to_string().contains("no reader prefix"), "{error}");
    }

    #[test]
    fn wrapping_then_unwrapping_returns_the_original() {
        let source = "(list x)";
        let wrapped = apply(source, "0.1", |input, tree, selection| {
            Edit::wrap_prefix(input, tree, selection, ReaderPrefix::UnquoteSplicing)
        })
        .unwrap();
        assert_eq!(wrapped, "(list ,@x)");
        let restored = apply(&wrapped, "0.1", |input, tree, selection| {
            Edit::unwrap_prefix(input, tree, selection, false)
        })
        .unwrap();
        assert_eq!(restored, source);
    }
}
