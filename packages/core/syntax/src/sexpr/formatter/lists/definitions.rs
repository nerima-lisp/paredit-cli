use crate::domain::sexpr::formatter::{Formatter, MAX_INLINE_WIDTH};
use crate::domain::sexpr::tree::{NodeKind, SyntaxTree};
use crate::domain::sexpr::types::NodeId;

#[derive(Clone, Copy)]
struct LastInlineChildPosition(Option<usize>);

impl Formatter {
    /// Formats ASDF `defsystem` forms, whose body is a name followed by a
    /// keyword/value option plist rather than a lambda list plus body forms.
    ///
    /// Unlike [`Formatter::format_definition`], the whole form is kept on one
    /// line when it fits the width budget, and when it must break each
    /// keyword/value pair stays together on its own line.
    pub(in crate::domain::sexpr::formatter) fn format_system_definition(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        if let Some(inline) = self.compact_form(tree, node_id) {
            output.push_str(&inline);
            return;
        }

        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        self.format_node(tree, node.children[0], depth + 1, output);
        if let Some(name) = node.children.get(1) {
            output.push(' ');
            self.format_inline_or_node(tree, *name, depth + 1, output);
        }

        for pair in node.children.get(2..).unwrap_or_default().chunks(2) {
            output.push('\n');
            output.push_str(&self.indent(depth + 1));
            self.format_inline_or_node(tree, pair[0], depth + 1, output);
            if let Some(value) = pair.get(1) {
                output.push(' ');
                self.format_inline_or_node(tree, *value, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    /// Compacts a whole list onto one line regardless of its head style,
    /// returning `None` when the result would exceed the width budget or any
    /// child cannot be compacted.
    fn compact_form(&self, tree: &SyntaxTree, node_id: NodeId) -> Option<String> {
        let node = tree.node(node_id);
        if self.is_opaque_reader_form(node) {
            return Some(node.span.slice(&tree.source).to_owned());
        }
        let delimiter = self.list_delimiter(node);
        let mut output = String::new();
        let reader_prefix_len = node
            .reader_prefix_spans
            .iter()
            .map(|span| span.slice(&tree.source).len())
            .sum::<usize>();
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            if position > 0 {
                output.push(' ');
            }
            output.push_str(&self.compact_node(tree, *child)?);
        }
        output.push(delimiter.close());
        (output.len().saturating_add(reader_prefix_len) <= MAX_INLINE_WIDTH).then_some(output)
    }

    pub(in crate::domain::sexpr::formatter) fn format_definition(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        self.format_definition_children(
            tree,
            node_id,
            depth,
            LastInlineChildPosition(Some(2)),
            output,
        );
    }

    pub(in crate::domain::sexpr::formatter) fn format_defmethod(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let last_inline_child_position = tree
            .node(node_id)
            .children
            .iter()
            .enumerate()
            .skip(2)
            .find_map(|(position, child)| {
                (tree.node(*child).kind == NodeKind::List).then_some(position)
            });

        self.format_definition_children(
            tree,
            node_id,
            depth,
            LastInlineChildPosition(last_inline_child_position),
            output,
        );
    }

    fn format_definition_children(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        last_inline_child_position: LastInlineChildPosition,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            if position == 0 {
                self.format_node(tree, *child, depth + 1, output);
            } else if last_inline_child_position
                .0
                .is_some_and(|last_inline| position <= last_inline)
            {
                output.push(' ');
                self.format_inline_or_node(tree, *child, depth + 1, output);
            } else {
                output.push('\n');
                output.push_str(&self.indent(depth + 1));
                self.format_node(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_prefix_body(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        prefix_len: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            if position <= prefix_len {
                if position > 0 {
                    output.push(' ');
                }
                self.format_inline_or_node(tree, *child, depth + 1, output);
            } else {
                output.push('\n');
                output.push_str(&self.indent(depth + 1));
                self.format_node(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_head_body(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            if position == 0 {
                self.format_node(tree, *child, depth + 1, output);
            } else {
                output.push('\n');
                output.push_str(&self.indent(depth + 1));
                self.format_node(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }
}
