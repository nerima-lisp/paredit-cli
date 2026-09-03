use crate::dialect::Dialect;
use crate::sexpr::formatter::Formatter;
use crate::sexpr::tree::{NodeKind, SyntaxTree};
use crate::sexpr::types::NodeId;

#[derive(Clone, Copy)]
enum ClauseFormKind {
    Do,
    Prog,
}

impl Formatter {
    pub(in crate::sexpr::formatter) fn format_clause_form(
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
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_clause(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_clause(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List
            || node.children.is_empty()
            || Self::carries_reader_prefix(node)
        {
            self.format_node(tree, node_id, depth, output);
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

    pub(in crate::sexpr::formatter) fn format_cond_clauses(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let base_column = Self::last_line_width(output);
        let body_column = if self.dialect == Dialect::EmacsLisp {
            base_column.saturating_add(1)
        } else {
            self.add_indent(base_column)
        };
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            if position == 0 {
                self.format_node(tree, *child, depth + 1, output);
            } else {
                Self::break_to_column(body_column, output);
                self.format_body_clause(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_case_clauses(
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
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_body_clause(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_body_clause(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List
            || node.children.len() <= 2
            || Self::carries_reader_prefix(node)
        {
            self.format_inline_or_node(tree, node_id, depth, output);
            return;
        }

        let delimiter = self.list_delimiter(node);
        let body_column = self.add_indent(Self::last_line_width(output));
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_inline_or_node(tree, *child, depth + 1, output),
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_do_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        self.format_clause_sequence_form(tree, node_id, depth, ClauseFormKind::Do, output);
    }

    pub(in crate::sexpr::formatter) fn format_prog_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        self.format_clause_sequence_form(tree, node_id, depth, ClauseFormKind::Prog, output);
    }

    fn format_clause_sequence_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        form_kind: ClauseFormKind,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let body_column = self.add_indent(Self::last_line_width(output));
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match (position, form_kind) {
                (0, _) => self.format_node(tree, *child, depth + 1, output),
                (1, _) => {
                    output.push(' ');
                    // Never aligned, regardless of `format.align-clause-values`:
                    // a `do`/`prog` var-clause may carry a third "step"
                    // element a two-column name/value layout does not fit,
                    // so this shape stays outside FR-013's scope (see
                    // `Formatter::format_sequence_list`'s doc comment).
                    self.format_sequence_list(tree, *child, depth + 1, false, output);
                }
                (2, ClauseFormKind::Do) => {
                    Self::break_to_column(body_column, output);
                    self.format_body_clause(tree, *child, depth + 1, output);
                }
                _ => {
                    Self::break_to_column(body_column, output);
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }
}
