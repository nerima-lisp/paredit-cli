use crate::domain::sexpr::formatter::Formatter;
use crate::domain::sexpr::tree::{NodeKind, SyntaxTree};
use crate::domain::sexpr::types::{Delimiter, NodeId};

impl Formatter {
    pub(in crate::domain::sexpr::formatter) fn format_general_list(
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

    pub(in crate::domain::sexpr::formatter) fn format_sequence_list(
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
        if delimiter == Delimiter::Bracket && node.children.len() % 2 == 0 {
            for (position, pair) in node.children.chunks_exact(2).enumerate() {
                if position > 0 {
                    output.push('\n');
                    output.push_str(&" ".repeat(continuation_column));
                }
                self.format_inline_or_node(tree, pair[0], depth + 1, output);
                output.push(' ');
                self.format_inline_or_node(tree, pair[1], depth + 1, output);
            }
            output.push(delimiter.close());
            return;
        }

        for (position, child) in node.children.iter().enumerate() {
            if position > 0 {
                output.push('\n');
                output.push_str(&" ".repeat(continuation_column));
            }
            self.format_inline_or_node(tree, *child, depth + 1, output);
        }
        output.push(delimiter.close());
    }
}
