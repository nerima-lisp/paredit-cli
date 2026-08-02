use crate::sexpr::formatter::Formatter;
use crate::sexpr::tree::{NodeKind, SyntaxTree};
use crate::sexpr::types::NodeId;

impl Formatter {
    pub(in crate::sexpr::formatter) fn format_binding_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        if self.head_text(tree, node_id).is_none() {
            self.format_general_list(tree, node_id, depth, output);
            return;
        }
        let body_column = self.add_indent(Self::last_line_width(output));
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_sequence_list(
                        tree,
                        *child,
                        depth + 1,
                        self.align_clause_values,
                        output,
                    );
                }
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_local_callable_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let body_column = self.add_indent(Self::last_line_width(output));
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_local_callable_bindings(tree, *child, depth + 1, output);
                }
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_local_callable_bindings(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List || node.children.is_empty() {
            self.format_inline_or_node(tree, node_id, depth, output);
            return;
        }

        let delimiter = self.list_delimiter(node);
        let entry_column = Self::last_line_width(output).saturating_add(1);
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            if position > 0 {
                Self::break_to_column(entry_column, output);
            }
            self.format_local_callable_binding(tree, *child, depth + 1, output);
        }
        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_local_callable_binding(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List || node.children.len() <= 2 {
            self.format_inline_or_node(tree, node_id, depth, output);
            return;
        }

        let delimiter = self.list_delimiter(node);
        let body_column = self.add_indent(Self::last_line_width(output));
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_declaration_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        // Measured from the head as written rather than from its byte
        // length, which is not a column count for a non-ASCII head.
        let mut continuation_column = 0;
        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => {
                    self.format_node(tree, *child, depth + 1, output);
                    continuation_column = Self::last_line_width(output).saturating_add(1);
                }
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    Self::break_to_column(continuation_column, output);
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_pair_assignment_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());
        self.format_node(tree, node.children[0], depth + 1, output);
        // One column past the head, i.e. under the first pair on the head
        // line — measured, not derived from the head's byte length.
        let continuation_column = Self::last_line_width(output).saturating_add(1);

        for (position, pair) in node.children[1..].chunks(2).enumerate() {
            if position == 0 {
                output.push(' ');
            } else {
                Self::break_to_column(continuation_column, output);
            }

            self.format_inline_or_node(tree, pair[0], depth + 1, output);
            if let Some(value) = pair.get(1) {
                output.push(' ');
                self.format_inline_or_node(tree, *value, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }
}
