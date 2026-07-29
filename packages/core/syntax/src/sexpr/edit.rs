use super::error::{SelectionError, SexprError, SexprResult, StructureError};
use super::tree::{Node, NodeKind, Selection, SyntaxTree};
use super::types::{ByteOffset, ByteSpan, Delimiter, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit;

impl Edit {
    pub fn normalize_changed_line_trivia(
        input: &str,
        rewritten: String,
        dialect: crate::dialect::Dialect,
    ) -> SexprResult<String> {
        if input == rewritten {
            return Ok(rewritten);
        }

        let tree = SyntaxTree::parse_with_dialect(&rewritten, dialect)?;
        let prefix = common_prefix_len(input, &rewritten);
        let suffix = common_suffix_len(input, &rewritten, prefix);
        let changed_end = rewritten.len().saturating_sub(suffix);
        let line_start = rewritten[..prefix]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let line_end = rewritten[changed_end..]
            .find('\n')
            .map_or(rewritten.len(), |newline| changed_end + newline + 1);

        let mut removals = Vec::new();
        let mut cursor = line_start;
        while cursor < line_end {
            let newline = rewritten[cursor..line_end]
                .find('\n')
                .map_or(line_end, |offset| cursor + offset);
            let content_end = if newline > cursor && rewritten.as_bytes()[newline - 1] == b'\r' {
                newline - 1
            } else {
                newline
            };
            let trailing_start = rewritten.as_bytes()[cursor..content_end]
                .iter()
                .rposition(|byte| !matches!(byte, b' ' | b'\t'))
                .map_or(cursor, |offset| cursor + offset + 1);

            if trailing_start < content_end
                && !trailing_trivia_is_opaque(&tree, trailing_start, content_end)
            {
                removals.push(trailing_start..content_end);
            }
            cursor = newline.saturating_add(1);
        }

        let mut normalized = rewritten;
        for removal in removals.into_iter().rev() {
            normalized.replace_range(removal, "");
        }
        Ok(normalized)
    }

    pub fn replace(
        input: &str,
        selection: Selection<'_>,
        replacement: &str,
    ) -> SexprResult<String> {
        validate_selection_input(input, selection)?;
        Ok(replace_span(input, selection.span(), replacement))
    }

    pub fn kill(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let span = expand_removal(input, tree, selection.span());
        Ok(replace_span(input, span, ""))
    }

    pub fn wrap(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        delimiter: Delimiter,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        Ok(replace_span(
            input,
            selection.span(),
            &format!(
                "{}{}{}",
                delimiter.open(),
                selection.text(),
                delimiter.close()
            ),
        ))
    }

    pub fn splice(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        ensure_list(node)?;
        let (open, close) = list_delimiter_offsets(node)?;
        let mut output = String::with_capacity(input.len().saturating_sub(2));
        output.push_str(&input[..open]);
        output.push_str(&input[open + 1..close]);
        output.push_str(&input[close + 1..]);
        Ok(output)
    }

    /// Replaces the selection's enclosing list with the selection.
    ///
    /// One level, which is what `paredit-raise-sexp` binds.
    /// [`Edit::raise_levels`] is the same operation with the count exposed.
    pub fn raise(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
        Self::raise_levels(input, tree, selection, 1)
    }

    pub fn transpose_forward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let sibling = next_sibling(tree, selection.node_id)
            .ok_or(StructureError::NoNextSiblingToTranspose)?;
        Ok(swap_node_text(
            input,
            selection.node().span,
            tree.node(sibling).span,
        ))
    }

    pub fn transpose_backward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let sibling = previous_sibling(tree, selection.node_id)
            .ok_or(StructureError::NoPreviousSiblingToTranspose)?;
        Ok(swap_node_text(
            input,
            tree.node(sibling).span,
            selection.node().span,
        ))
    }

    pub fn slurp_forward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        ensure_list(node)?;
        let sibling =
            next_sibling(tree, selection.node_id).ok_or(StructureError::NoNextSiblingToSlurp)?;
        let (_, close) = list_delimiter_offsets(node)?;
        let insertion = format!(" {}", tree.node(sibling).span.slice(input));
        // The sibling sits after the list, so the gap to remove is the
        // whitespace between the closing delimiter and the sibling. Absorbing
        // trailing whitespace instead would eat a document-terminating newline
        // and strand the list-facing gap as a dangling space.
        let removal = expand_removal_leading(input, tree, tree.node(sibling).span);
        Ok(remove_then_insert(
            input,
            removal,
            ByteOffset::new(close),
            &insertion,
        ))
    }

    pub fn slurp_backward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        ensure_list(node)?;
        let sibling = previous_sibling(tree, selection.node_id)
            .ok_or(StructureError::NoPreviousSiblingToSlurp)?;
        let (open, _) = list_delimiter_offsets(node)?;
        let open = open + 1;
        let insertion = format!("{} ", tree.node(sibling).span.slice(input));
        let removal = expand_removal(input, tree, tree.node(sibling).span);
        Ok(remove_then_insert(
            input,
            removal,
            ByteOffset::new(open),
            &insertion,
        ))
    }

    pub fn barf_forward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        ensure_list(node)?;
        let child = *node
            .children
            .last()
            .ok_or(StructureError::BarfFromEmptyList)?;
        let (_, close) = list_delimiter_offsets(node)?;
        let child_span = tree.node(child).span;
        let insertion = format!(" {}", child_span.slice(input));
        let removal = expand_removal(input, tree, child_span);
        Ok(remove_then_insert(
            input,
            removal,
            ByteOffset::new(close + 1),
            &insertion,
        ))
    }

    pub fn barf_backward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        ensure_list(node)?;
        let child = *node
            .children
            .first()
            .ok_or(StructureError::BarfFromEmptyList)?;
        let open = node.open.ok_or(StructureError::MissingOpenDelimiter)?;
        let child_span = tree.node(child).span;
        let insertion = format!("{} ", child_span.slice(input));
        let removal = expand_removal(input, tree, child_span);
        Ok(remove_then_insert(input, removal, open, &insertion))
    }

    /// Split the enclosing list immediately before the selected expression,
    /// producing two sibling lists that share the original delimiter. The
    /// selection becomes the first child of the trailing list.
    ///
    /// `(foo bar baz qux)` selecting `baz` yields `(foo bar) (baz qux)`.
    ///
    /// The gap between the split point's neighbours is preserved verbatim so
    /// interleaved comments survive; the caller's trivia normalization then
    /// trims any whitespace stranded on the changed lines.
    pub fn split(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        let parent_id = node.parent.ok_or(StructureError::NoEnclosingListToSplit)?;
        let parent = tree.node(parent_id);
        if parent.kind != NodeKind::List {
            return Err(StructureError::SplitNotDirectlyInList.into());
        }
        if !parent.reader_prefixes.is_empty() {
            return Err(StructureError::SplitReaderPrefix.into());
        }
        let delimiter = parent
            .delimiter
            .ok_or(StructureError::EnclosingListMissingDelimiter)?;
        let previous = previous_sibling(tree, selection.node_id)
            .ok_or(StructureError::SplitBeforeFirstElement)?;
        let prev_end = tree.node(previous).span.end().get();
        let selection_start = node.span.start().get();

        let mut output = String::with_capacity(input.len() + 2);
        output.push_str(&input[..prev_end]);
        output.push(delimiter.close());
        output.push_str(&input[prev_end..selection_start]);
        output.push(delimiter.open());
        output.push_str(&input[selection_start..]);
        Ok(output)
    }

    /// Join the selection with its next sibling. Two lists merge into one list
    /// concatenating their children; two string literals merge into one string
    /// concatenating their contents. Adjacent symbols are refused because
    /// fusing them silently changes tokenization.
    ///
    /// `(foo bar) (baz qux)` selecting the first list yields `(foo bar baz qux)`.
    /// `"foo" "bar"` selecting the first string yields `"foobar"`.
    pub fn join(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        let sibling_id =
            next_sibling(tree, selection.node_id).ok_or(StructureError::NoNextSiblingToJoin)?;
        let sibling = tree.node(sibling_id);

        if node.kind == NodeKind::Atom {
            return join_strings(input, node, sibling);
        }

        ensure_list(node)?;
        if !node.reader_prefixes.is_empty() {
            return Err(StructureError::JoinReaderPrefix.into());
        }
        if sibling.kind != NodeKind::List {
            return Err(StructureError::JoinSiblingNotList.into());
        }
        if !sibling.reader_prefixes.is_empty() {
            return Err(StructureError::JoinIntoReaderPrefix.into());
        }
        if node.delimiter != sibling.delimiter {
            return Err(StructureError::JoinDelimiterMismatch.into());
        }
        let (_, first_close) = list_delimiter_offsets(node)?;
        let (second_open, _) = list_delimiter_offsets(sibling)?;
        let gap = &input[first_close + 1..second_open];
        let separator =
            if gap.is_empty() && !node.children.is_empty() && !sibling.children.is_empty() {
                " "
            } else {
                gap
            };

        let mut output = String::with_capacity(input.len());
        output.push_str(&input[..first_close]);
        output.push_str(separator);
        output.push_str(&input[second_open + 1..]);
        Ok(output)
    }

    /// Splice the enclosing list, killing every sibling *before* the selection.
    /// The selection and everything after it survive verbatim; the list's
    /// delimiters and preceding siblings are removed.
    ///
    /// `(let ((x 5)) (foo x) bar)` selecting `(foo x)` yields `(foo x) bar`.
    pub fn splice_killing_backward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        let parent = enclosing_list(tree, node)?;
        let last = *parent
            .children
            .last()
            .ok_or(StructureError::EnclosingListHasNoChildren)?;
        let start = node.span.start().get();
        let end = tree.node(last).span.end().get();
        Ok(replace_span(input, parent.span, &input[start..end]))
    }

    /// Splice the enclosing list, killing the selection and every sibling
    /// *after* it. The siblings before the selection survive verbatim; the
    /// list's delimiters and the trailing siblings are removed.
    ///
    /// `(foo (bar) baz qux)` selecting `baz` yields `foo (bar)`.
    pub fn splice_killing_forward(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let node = selection.node();
        let parent = enclosing_list(tree, node)?;
        let previous = previous_sibling(tree, selection.node_id)
            .ok_or(StructureError::NothingPrecedesSelection)?;
        let first = *parent
            .children
            .first()
            .ok_or(StructureError::EnclosingListHasNoChildren)?;
        let start = tree.node(first).span.start().get();
        let end = tree.node(previous).span.end().get();
        Ok(replace_span(input, parent.span, &input[start..end]))
    }

    /// Convolute the two lists enclosing the selected list, reversing which one
    /// nests inside the other. The selected list stays innermost as the anchor.
    ///
    /// `(let ((x 1)) (foo (bar baz) quux))` selecting `(bar baz)` yields
    /// `(foo (let ((x 1)) (bar baz)) quux)`.
    ///
    /// Only trivia *between* siblings is normalized to single spaces; comments
    /// living inside any moved form survive because those forms are sliced
    /// verbatim. To avoid silently dropping a comment that sits between the
    /// reshuffled forms, the operation refuses any comment inside the outer
    /// list that is not inside the selected list.
    pub fn convolute(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        let inner = selection.node();
        ensure_list(inner)?;
        let middle_id = inner
            .parent
            .ok_or(StructureError::NoEnclosingListToConvolute)?;
        let middle = tree.node(middle_id);
        if middle.kind != NodeKind::List {
            return Err(StructureError::ConvoluteNotNested.into());
        }
        let outer_id = middle.parent.ok_or(StructureError::ConvoluteNotTwoDeep)?;
        let outer = tree.node(outer_id);
        if outer.kind != NodeKind::List {
            return Err(StructureError::ConvoluteNotTwoDeep.into());
        }
        if !middle.reader_prefixes.is_empty() || !outer.reader_prefixes.is_empty() {
            return Err(StructureError::ConvoluteReaderPrefix.into());
        }
        let middle_delimiter = middle
            .delimiter
            .ok_or(StructureError::EnclosingListMissingDelimiter)?;
        let outer_delimiter = outer
            .delimiter
            .ok_or(StructureError::OuterListMissingDelimiter)?;

        let inner_span = inner.span;
        let outer_span = outer.span;
        if tree.comments.iter().any(|comment| {
            let within_outer = comment.span.start().get() >= outer_span.start().get()
                && comment.span.end().get() <= outer_span.end().get();
            let within_inner = comment.span.start().get() >= inner_span.start().get()
                && comment.span.end().get() <= inner_span.end().get();
            within_outer && !within_inner
        }) {
            return Err(StructureError::ConvoluteCommentsOutside.into());
        }

        let inner_position = middle
            .children
            .iter()
            .position(|child| *child == selection.node_id)
            .ok_or(StructureError::NotDirectChildOfEnclosing)?;
        let middle_position = outer
            .children
            .iter()
            .position(|child| *child == middle_id)
            .ok_or(StructureError::EnclosingNotDirectChildOfOuter)?;

        let middle_before = &middle.children[..inner_position];
        let middle_after = &middle.children[inner_position + 1..];
        let outer_before = &outer.children[..middle_position];
        let outer_after = &outer.children[middle_position + 1..];

        let join = |ids: &[NodeId]| {
            ids.iter()
                .map(|id| tree.node(*id).span.slice(input))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let middle_before = join(middle_before);
        let middle_after = join(middle_after);
        let outer_before = join(outer_before);
        let outer_after = join(outer_after);
        let inner_text = inner_span.slice(input);

        let mut relocated_outer = String::new();
        relocated_outer.push(outer_delimiter.open());
        push_space_joined(
            &mut relocated_outer,
            &[&outer_before, inner_text, &outer_after],
        );
        relocated_outer.push(outer_delimiter.close());

        let mut rewritten = String::new();
        rewritten.push(middle_delimiter.open());
        push_space_joined(
            &mut rewritten,
            &[&middle_before, &relocated_outer, &middle_after],
        );
        rewritten.push(middle_delimiter.close());

        Ok(replace_span(input, outer.span, &rewritten))
    }
}

/// Merges two adjacent string-literal atoms into one string by concatenating
/// their contents and dropping the interior delimiters and the gap between
/// them. Refuses non-string atoms so symbols are never silently fused.
fn join_strings(input: &str, node: &Node, sibling: &Node) -> SexprResult<String> {
    if !node.reader_prefixes.is_empty() || !sibling.reader_prefixes.is_empty() {
        return Err(StructureError::JoinStringReaderPrefix.into());
    }
    let first = node.span.slice(input);
    let second = sibling.span.slice(input);
    if !is_string_literal(first) || sibling.kind != NodeKind::Atom || !is_string_literal(second) {
        return Err(StructureError::JoinUnsupportedPair.into());
    }

    // Drop the first string's closing quote and the second's opening quote so
    // their contents abut inside a single pair of quotes: `"foo` + `bar"`.
    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..node.span.end().get() - 1]);
    output.push_str(&second[1..]);
    output.push_str(&input[sibling.span.end().get()..]);
    Ok(output)
}

/// Reports whether `text` is a double-quoted string literal (`"..."`).
pub(in crate::sexpr) fn is_string_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'
}

/// Returns the selected node's enclosing list, or an error when the node is at
/// the top level or otherwise not directly inside a list.
fn enclosing_list<'a>(tree: &'a SyntaxTree, node: &Node) -> SexprResult<&'a Node> {
    let parent_id = node
        .parent
        .ok_or(StructureError::NoEnclosingListForSelection)?;
    let parent = tree.node(parent_id);
    if parent.kind != NodeKind::List {
        return Err(StructureError::NotInsideList.into());
    }
    Ok(parent)
}

/// Appends the non-empty `parts` to `out`, separated by single spaces.
fn push_space_joined(out: &mut String, parts: &[&str]) {
    let mut first = true;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if !first {
            out.push(' ');
        }
        out.push_str(part);
        first = false;
    }
}

fn validate_selection_input(input: &str, selection: Selection<'_>) -> SexprResult<()> {
    selection.validate_source(input).map_err(prefix_with_edit)
}

pub(in crate::sexpr) fn validate_edit_context(
    input: &str,
    tree: &SyntaxTree,
    selection: Selection<'_>,
) -> SexprResult<()> {
    selection.validate_context(input, tree).map_err(|error| {
        // Only a source mismatch names the operation. This used to be
        // `error.to_string().starts_with("input ")` - a prefix match on a
        // human message deciding control flow.
        match error {
            SelectionError::SourceMismatch => prefix_with_edit(SelectionError::SourceMismatch),
            other => SexprError::Selection(other),
        }
    })
}

/// Names the operation in a selection failure, as the CLI's messages expect.
const fn prefix_with_edit(source: SelectionError) -> SexprError {
    SexprError::EditSelection { source }
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    let mut length = left
        .as_bytes()
        .iter()
        .zip(right.as_bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while !left.is_char_boundary(length) || !right.is_char_boundary(length) {
        length -= 1;
    }
    length
}

fn common_suffix_len(left: &str, right: &str, prefix: usize) -> usize {
    let max = left.len().min(right.len()).saturating_sub(prefix);
    let mut length = left.as_bytes()[left.len() - max..]
        .iter()
        .rev()
        .zip(right.as_bytes()[right.len() - max..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    while !left.is_char_boundary(left.len() - length)
        || !right.is_char_boundary(right.len() - length)
    {
        length -= 1;
    }
    length
}

fn trailing_trivia_is_opaque(tree: &SyntaxTree, start: usize, end: usize) -> bool {
    tree.nodes.iter().any(|node| {
        node.kind == NodeKind::Atom
            && node.span.start().get() < end
            && start < node.span.end().get()
    }) || tree.comments.iter().any(|comment| {
        !comment.text.starts_with(';')
            && comment.span.start().get() < end
            && start < comment.span.end().get()
    })
}

fn ensure_list(node: &Node) -> SexprResult<()> {
    if node.kind != NodeKind::List {
        return Err(StructureError::NotAList.into());
    }
    Ok(())
}

fn list_delimiter_offsets(node: &Node) -> SexprResult<(usize, usize)> {
    let open = node.open.ok_or(StructureError::MissingOpenDelimiter)?;
    let close = node.close.ok_or(StructureError::MissingCloseDelimiter)?;
    Ok((open.get(), close.get()))
}

pub(in crate::sexpr) fn next_sibling(tree: &SyntaxTree, node_id: NodeId) -> Option<NodeId> {
    let parent = tree.node(node_id).parent?;
    let siblings = &tree.node(parent).children;
    let position = siblings.iter().position(|id| *id == node_id)?;
    siblings.get(position + 1).copied()
}

pub(in crate::sexpr) fn previous_sibling(tree: &SyntaxTree, node_id: NodeId) -> Option<NodeId> {
    let parent = tree.node(node_id).parent?;
    let siblings = &tree.node(parent).children;
    let position = siblings.iter().position(|id| *id == node_id)?;
    position
        .checked_sub(1)
        .and_then(|previous| siblings.get(previous).copied())
}

pub(in crate::sexpr) fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

pub(in crate::sexpr) fn swap_node_text(input: &str, left: ByteSpan, right: ByteSpan) -> String {
    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..left.start().get()]);
    output.push_str(right.slice(input));
    // Trivia belongs to its structural slot, not to either expression.
    output.push_str(&input[left.end().get()..right.start().get()]);
    output.push_str(left.slice(input));
    output.push_str(&input[right.end().get()..]);
    output
}

fn expand_removal(input: &str, tree: &SyntaxTree, span: ByteSpan) -> ByteSpan {
    let bytes = input.as_bytes();
    let mut start = span.start().get();
    let mut end = span.end().get();
    if end < bytes.len() && bytes[end].is_ascii_whitespace() {
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
    } else {
        // A comment ends right before the newline that terminates it; that
        // newline is load-bearing — deleting it would splice whatever
        // follows onto the comment's line, commenting it out. Never absorb
        // whitespace back past the byte immediately after a comment.
        let floor = tree
            .comments
            .iter()
            .map(|comment| comment.span.end().get())
            .filter(|comment_end| *comment_end < start)
            .max()
            .map_or(0, |comment_end| comment_end + 1);
        while start > floor && bytes[start - 1].is_ascii_whitespace() {
            start -= 1;
        }
    }
    ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
}

fn expand_removal_leading(input: &str, tree: &SyntaxTree, span: ByteSpan) -> ByteSpan {
    let bytes = input.as_bytes();
    let mut start = span.start().get();
    let end = span.end().get();
    let floor = tree
        .comments
        .iter()
        .map(|comment| comment.span.end().get())
        .filter(|comment_end| *comment_end < start)
        .max()
        .map_or(0, |comment_end| comment_end + 1);
    while start > floor && bytes[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
}

fn remove_then_insert(
    input: &str,
    removal: ByteSpan,
    insertion_at: ByteOffset,
    insertion: &str,
) -> String {
    let adjusted_insertion_at = if insertion_at.get() > removal.end().get() {
        insertion_at.get() - removal.len()
    } else {
        insertion_at.get()
    };
    let removed = replace_span(input, removal, "");
    replace_span(
        &removed,
        ByteSpan::new(
            ByteOffset::new(adjusted_insertion_at),
            ByteOffset::new(adjusted_insertion_at),
        ),
        insertion,
    )
}
