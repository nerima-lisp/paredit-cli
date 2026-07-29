//! The structural edits that need more than one selection, or more than one
//! level, to say what they mean.
//!
//! `edit raise` climbs exactly one level and `edit transpose-forward` swaps
//! exactly the adjacent sibling, which is right for a keystroke and limiting
//! for a command: raising out of `(when x (let ((y 1)) (f y)))` takes two
//! calls, and the second one has to be planned against a document the first
//! one changed. The variants here take the count and the second address up
//! front, so one call does what the editor needs several for.

use crate::leading_trivia::first_newline_or;

use super::edit::{Edit, previous_sibling, replace_span, swap_node_text, validate_edit_context};
use super::error::{SexprResult, StructureError};
use super::tree::{NodeKind, Selection, SyntaxTree};
use super::types::{ByteOffset, ByteSpan};

/// Where a yanked form is placed relative to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Before,
    After,
}

impl Edit {
    /// Replaces the `levels`-th enclosing list with the selected expression.
    ///
    /// `levels == 1` is [`Edit::raise`]. Climbing further is not the same as
    /// repeating it: each intermediate list is discarded whole, and the refusal
    /// names how deep the selection actually sits so a caller can retry with a
    /// number that exists.
    pub fn raise_levels(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        levels: usize,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let mut target = selection.node_id;
        for climbed in 0..levels.max(1) {
            let parent = tree.node(target).parent.ok_or(StructureError::NoParent)?;
            if tree.node(parent).kind == NodeKind::Root {
                return Err(if climbed == 0 {
                    StructureError::RaiseTopLevel
                } else {
                    StructureError::RaiseLevelsExceedDepth {
                        requested: levels,
                        available: climbed,
                    }
                }
                .into());
            }
            target = parent;
        }
        Ok(replace_span(
            input,
            tree.node(target).span,
            selection.text(),
        ))
    }

    /// Exchanges two expressions that share an enclosing list, adjacent or not.
    ///
    /// The trivia between them stays where it is, exactly as the adjacent
    /// transposes leave it: a comment belongs to the slot it was written
    /// beside, not to the expression that happened to be there.
    pub fn transpose_siblings(
        input: &str,
        tree: &SyntaxTree,
        first: Selection<'_>,
        second: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, first)?;
        validate_edit_context(input, tree, second)?;
        if first.node_id == second.node_id {
            return Err(StructureError::TransposeSameExpression.into());
        }
        let (Some(left_parent), Some(right_parent)) = (first.node().parent, second.node().parent)
        else {
            return Err(StructureError::TransposeNotSiblings.into());
        };
        if left_parent != right_parent {
            return Err(StructureError::TransposeNotSiblings.into());
        }

        let (left, right) = if first.span().start().get() <= second.span().start().get() {
            (first.span(), second.span())
        } else {
            (second.span(), first.span())
        };
        Ok(swap_node_text(input, left, right))
    }

    /// Returns the selected expression together with the own-line comments
    /// written directly above it.
    ///
    /// The read-only half of [`Edit::kill`], and the reason it is not
    /// `edit select`: `select` answers with the form's span, which stops at the
    /// opening delimiter and leaves the `;;` block that documents it behind.
    /// Pasting that back produces an undocumented copy.
    pub fn copy(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let start = selection.span().start().get();
        let trivia_start = leading_trivia_start(input, tree, selection);
        if trivia_start == start
            || !tree.has_comment_in(ByteSpan::new(
                ByteOffset::new(trivia_start),
                ByteOffset::new(start),
            ))
        {
            return Ok(selection.text().to_owned());
        }
        Ok(format!(
            "{}{}",
            skip_blank_lines(&input[trivia_start..start]),
            selection.text()
        ))
    }

    /// Inserts `text` as a new sibling beside the selection.
    ///
    /// The separator follows the selection's own layout: a form that starts its
    /// line gets the new sibling on a line of its own at the same column, and
    /// one that shares a line gets a single space. Anything cleverer would need
    /// to know why the author broke the line where they did.
    pub fn insert_sibling(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        text: &str,
        placement: Placement,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let start = selection.span().start().get();
        let separator = match line_indent(input, start) {
            Some(indent) => format!("\n{}", " ".repeat(indent)),
            None => " ".to_owned(),
        };

        let (at, insertion) = match placement {
            Placement::Before => (start, format!("{text}{separator}")),
            Placement::After => (selection.span().end().get(), format!("{separator}{text}")),
        };
        let mut output = String::with_capacity(input.len() + insertion.len());
        output.push_str(&input[..at]);
        output.push_str(&insertion);
        output.push_str(&input[at..]);
        Ok(output)
    }
}

/// Where the selection's own leading trivia begins: just past the newline that
/// ends whatever precedes it, or the selection itself when they share a line.
fn leading_trivia_start(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> usize {
    let start = selection.span().start().get();
    let node = selection.node();
    let previous_end = previous_sibling(tree, selection.node_id)
        .map(|sibling| tree.node(sibling).span.end().get())
        .or_else(|| {
            let parent = node.parent?;
            tree.node(parent).open.map(|open| open.get() + 1)
        })
        .unwrap_or(0);
    if previous_end >= start {
        return start;
    }
    let newline = first_newline_or(input, previous_end, start);
    if newline < start && input.as_bytes()[newline] == b'\n' {
        newline + 1
    } else {
        start
    }
}

/// Drops the leading whitespace-only lines from a run of trivia.
fn skip_blank_lines(trivia: &str) -> &str {
    let mut cut = 0usize;
    for line in trivia.split_inclusive('\n') {
        if line.ends_with('\n') && line.trim().is_empty() {
            cut += line.len();
        } else {
            break;
        }
    }
    &trivia[cut..]
}

/// The indentation of `offset`'s line when only whitespace precedes it there.
fn line_indent(input: &str, offset: usize) -> Option<usize> {
    let line_start = input[..offset].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &input[line_start..offset];
    prefix
        .chars()
        .all(char::is_whitespace)
        .then(|| prefix.chars().count())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sexpr::ExpressionPath;

    fn select<'a>(tree: &'a SyntaxTree, path: &str) -> Selection<'a> {
        tree.select_path(&path.parse::<ExpressionPath>().unwrap())
            .unwrap()
    }

    #[test]
    fn raise_levels_discards_every_intermediate_list() {
        let source = "(when x (let ((y 1)) (f y)))";
        let tree = SyntaxTree::parse(source).unwrap();
        let selection = select(&tree, "0.2.2");
        assert_eq!(
            Edit::raise_levels(source, &tree, selection, 1).unwrap(),
            "(when x (f y))"
        );
        assert_eq!(
            Edit::raise_levels(source, &tree, selection, 2).unwrap(),
            "(f y)"
        );
    }

    #[test]
    fn raise_levels_names_the_depth_it_actually_had() {
        let source = "(when x (f y))";
        let tree = SyntaxTree::parse(source).unwrap();
        let error = Edit::raise_levels(source, &tree, select(&tree, "0.2"), 4).unwrap_err();
        assert!(error.to_string().contains("only 1 levels deep"), "{error}");
    }

    #[test]
    fn raise_levels_of_one_keeps_the_top_level_refusal() {
        let source = "(f y)";
        let tree = SyntaxTree::parse(source).unwrap();
        let error = Edit::raise_levels(source, &tree, select(&tree, "0"), 1).unwrap_err();
        assert!(error.to_string().contains("top-level"), "{error}");
    }

    #[test]
    fn transpose_siblings_swaps_across_a_gap() {
        let source = "(alpha beta gamma delta)";
        let tree = SyntaxTree::parse(source).unwrap();
        let output =
            Edit::transpose_siblings(source, &tree, select(&tree, "0.1"), select(&tree, "0.3"))
                .unwrap();
        assert_eq!(output, "(alpha delta gamma beta)");
    }

    #[test]
    fn transpose_siblings_is_order_independent() {
        let source = "(alpha beta gamma delta)";
        let tree = SyntaxTree::parse(source).unwrap();
        let forward =
            Edit::transpose_siblings(source, &tree, select(&tree, "0.1"), select(&tree, "0.3"))
                .unwrap();
        let backward =
            Edit::transpose_siblings(source, &tree, select(&tree, "0.3"), select(&tree, "0.1"))
                .unwrap();
        assert_eq!(forward, backward);
    }

    #[test]
    fn transpose_siblings_refuses_two_forms_in_different_lists() {
        let source = "((a b) (c d))";
        let tree = SyntaxTree::parse(source).unwrap();
        let error = Edit::transpose_siblings(
            source,
            &tree,
            select(&tree, "0.0.0"),
            select(&tree, "0.1.0"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("same list"), "{error}");
    }

    #[test]
    fn transpose_siblings_refuses_a_form_and_itself() {
        let source = "(a b)";
        let tree = SyntaxTree::parse(source).unwrap();
        let error =
            Edit::transpose_siblings(source, &tree, select(&tree, "0.0"), select(&tree, "0.0"))
                .unwrap_err();
        assert!(error.to_string().contains("with itself"), "{error}");
    }

    #[test]
    fn copy_takes_the_comment_block_written_above_the_form() {
        let source = "(progn\n  ;; explain\n  (f y)\n  (g z))";
        let tree = SyntaxTree::parse(source).unwrap();
        let copied = Edit::copy(source, &tree, select(&tree, "0.1")).unwrap();
        assert_eq!(copied, "  ;; explain\n  (f y)");
    }

    #[test]
    fn copy_of_a_form_with_no_comment_is_exactly_its_text() {
        let source = "(progn\n  (f y))";
        let tree = SyntaxTree::parse(source).unwrap();
        assert_eq!(
            Edit::copy(source, &tree, select(&tree, "0.1")).unwrap(),
            "(f y)"
        );
    }

    #[test]
    fn copy_leaves_the_document_untouched() {
        let source = "(progn\n  ;; explain\n  (f y))";
        let tree = SyntaxTree::parse(source).unwrap();
        Edit::copy(source, &tree, select(&tree, "0.1")).unwrap();
        assert_eq!(tree.source(), source);
    }

    #[test]
    fn insert_sibling_follows_the_selection_layout() {
        let inline = "(list a b)";
        let tree = SyntaxTree::parse(inline).unwrap();
        assert_eq!(
            Edit::insert_sibling(inline, &tree, select(&tree, "0.1"), "z", Placement::After)
                .unwrap(),
            "(list a z b)"
        );

        let stacked = "(progn\n  (f y)\n  (g z))";
        let tree = SyntaxTree::parse(stacked).unwrap();
        assert_eq!(
            Edit::insert_sibling(
                stacked,
                &tree,
                select(&tree, "0.1"),
                "(h w)",
                Placement::Before
            )
            .unwrap(),
            "(progn\n  (h w)\n  (f y)\n  (g z))"
        );
    }
}
