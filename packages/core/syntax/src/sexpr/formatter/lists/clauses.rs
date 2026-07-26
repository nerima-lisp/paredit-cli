use crate::domain::sexpr::formatter::Formatter;
use crate::domain::sexpr::tree::{NodeKind, SyntaxTree};
use crate::domain::sexpr::types::NodeId;

#[derive(Clone, Copy)]
enum ClauseFormKind {
    Do,
    Prog,
}

impl Formatter {
    pub(in crate::domain::sexpr::formatter) fn format_clause_form(
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
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    output.push('\n');
                    output.push_str(&self.indent(depth + 1));
                    self.format_clause(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_clause(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List || node.children.is_empty() {
            self.format_node(tree, node_id, depth, output);
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
                    output.push_str(&self.indent(depth + 1));
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_cond_clauses(
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
                self.format_body_clause(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_case_clauses(
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
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    output.push('\n');
                    output.push_str(&self.indent(depth + 1));
                    self.format_body_clause(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_body_clause(
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
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_inline_or_node(tree, *child, depth + 1, output),
                _ => {
                    output.push('\n');
                    output.push_str(&self.indent(depth + 1));
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_do_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        output: &mut String,
    ) {
        self.format_clause_sequence_form(tree, node_id, depth, head, ClauseFormKind::Do, output);
    }

    pub(in crate::domain::sexpr::formatter) fn format_prog_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        output: &mut String,
    ) {
        self.format_clause_sequence_form(tree, node_id, depth, head, ClauseFormKind::Prog, output);
    }

    fn format_clause_sequence_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        form_kind: ClauseFormKind,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match (position, form_kind) {
                (0, _) => self.format_node(tree, *child, depth + 1, output),
                (1, _) => {
                    output.push(' ');
                    self.format_sequence_list(
                        tree,
                        *child,
                        depth + 1,
                        self.continuation_column(depth, head.len().saturating_add(3)),
                        output,
                    );
                }
                (2, ClauseFormKind::Do) => {
                    output.push('\n');
                    output.push_str(&self.indent(depth + 1));
                    self.format_body_clause(tree, *child, depth + 1, output);
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
}
