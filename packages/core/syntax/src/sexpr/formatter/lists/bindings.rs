use crate::domain::sexpr::formatter::Formatter;
use crate::domain::sexpr::tree::{NodeKind, SyntaxTree};
use crate::domain::sexpr::types::NodeId;

impl Formatter {
    pub(in crate::domain::sexpr::formatter) fn format_binding_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let Some(head) = self.head_text(tree, node_id) else {
            self.format_general_list(tree, node_id, depth, output);
            return;
        };
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
                        self.continuation_column(depth, head.len().saturating_add(3)),
                        output,
                    );
                }
                _ => {
                    output.push('\n');
                    output.push_str(&self.indent(depth + 1));
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_local_callable_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_local_callable_bindings(
                        tree,
                        *child,
                        depth + 1,
                        self.continuation_column(depth, head.len().saturating_add(3)),
                        output,
                    );
                }
                _ => {
                    output.push('\n');
                    output.push_str(&self.indent(depth + 1));
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_local_callable_bindings(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        continuation_column: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List || node.children.is_empty() {
            self.format_inline_or_node(tree, node_id, depth, output);
            return;
        }

        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            if position > 0 {
                output.push('\n');
                output.push_str(&" ".repeat(continuation_column));
            }
            self.format_local_callable_binding(
                tree,
                *child,
                depth + 1,
                self.add_indent(continuation_column),
                output,
            );
        }
        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_local_callable_binding(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        body_column: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List || node.children.len() <= 2 {
            self.format_inline_or_node(tree, node_id, depth, output);
            return;
        }

        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    output.push('\n');
                    output.push_str(&" ".repeat(body_column));
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_declaration_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let continuation_column = self.continuation_column(depth, head.len().saturating_add(2));
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    output.push('\n');
                    output.push_str(&" ".repeat(continuation_column));
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_pair_assignment_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let continuation_column = self.continuation_column(depth, head.len().saturating_add(2));
        output.push(delimiter.open());
        self.format_node(tree, node.children[0], depth + 1, output);

        for (position, pair) in node.children[1..].chunks(2).enumerate() {
            if position == 0 {
                output.push(' ');
            } else {
                output.push('\n');
                output.push_str(&" ".repeat(continuation_column));
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
