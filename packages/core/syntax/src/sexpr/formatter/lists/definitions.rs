use crate::sexpr::formatter::Formatter;
use crate::sexpr::formatter::core::Bounded;
use crate::sexpr::tree::{NodeKind, SyntaxTree};
use crate::sexpr::types::NodeId;

#[derive(Clone, Copy)]
struct LastInlineChildPosition(Option<usize>);

impl Formatter {
    /// Formats ASDF `defsystem` forms, whose body is a name followed by a
    /// keyword/value option plist rather than a lambda list plus body forms.
    ///
    /// Unlike [`Formatter::format_definition`], the whole form is kept on one
    /// line when it fits the width budget, and when it must break each
    /// keyword/value pair stays together on its own line.
    pub(in crate::sexpr::formatter) fn format_system_definition(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let base_column = Self::last_line_width(output);
        if let Some(inline) = self.compact_form(tree, node_id, base_column) {
            output.push_str(&inline);
            return;
        }

        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let body_column = self.add_indent(base_column);
        output.push(delimiter.open());

        self.format_node(tree, node.children[0], depth + 1, output);
        if let Some(name) = node.children.get(1) {
            output.push(' ');
            self.format_inline_or_node(tree, *name, depth + 1, output);
        }

        for pair in node.children.get(2..).unwrap_or_default().chunks(2) {
            Self::break_to_column(body_column, output);
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
    ///
    /// `start_column` is the display column this rendering would be written
    /// at, so that `max_width` stays a line width rather than a per-form
    /// allowance. It already accounts for any reader prefix: those are
    /// written by [`Formatter::format_node`] before it dispatches on the
    /// head's style, so they are behind the cursor the caller measured.
    ///
    /// Builds against a running [`Bounded`] width instead of accumulating a
    /// plain `String` and re-scanning it for its display width at the end —
    /// the same incremental-width pattern [`Formatter::compact_node`] uses,
    /// and it lets a form that is already over budget bail out before
    /// compacting its remaining children.
    pub(in crate::sexpr::formatter) fn compact_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        start_column: usize,
    ) -> Option<String> {
        let node = tree.node(node_id);
        if self.is_opaque_reader_form(node) {
            return Some(node.span.slice(&tree.source).to_owned());
        }
        let delimiter = self.list_delimiter(node);
        // `self.max_width`, not the compiled-in constant: this was the last
        // width decision `--max-width` did not reach, which left a `defsystem`
        // header inlined up to 80 columns under `--max-width 40` and broken at
        // 80 under `--max-width 120`. Measured in display columns rather than
        // bytes for the same reason `compact_node` is: a CJK-heavy option
        // value is narrower in bytes than it is on screen.
        let max_width = self.max_width;
        let mut output = Bounded::with_initial_width(start_column);
        output.push_char(delimiter.open(), max_width)?;
        for (position, child) in node.children.iter().enumerate() {
            if position > 0 {
                output.push_char(' ', max_width)?;
            }
            output.push_str(&self.compact_node(tree, *child, 0)?, max_width)?;
        }
        output.push_char(delimiter.close(), max_width)?;
        Some(output.into_text())
    }

    pub(in crate::sexpr::formatter) fn format_definition(
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

    pub(in crate::sexpr::formatter) fn format_defmethod(
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
        let body_column = self.add_indent(Self::last_line_width(output));
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
                self.break_between_nodes(
                    tree,
                    node.children[position - 1],
                    *child,
                    body_column,
                    output,
                );
                self.format_node(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_prefix_body(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        prefix_len: usize,
        output: &mut String,
    ) {
        self.format_prefix_body_children(tree, node_id, depth, prefix_len, false, output);
    }

    pub(in crate::sexpr::formatter) fn format_lambda(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        prefix_len: usize,
        output: &mut String,
    ) {
        self.format_prefix_body_children(tree, node_id, depth, prefix_len, true, output);
    }

    fn format_prefix_body_children(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        prefix_len: usize,
        compact_distinguished_list: bool,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let base_column = Self::last_line_width(output);
        let body_column = self.add_indent(base_column);
        let distinguished_column = self.add_indent(body_column);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            if position < prefix_len {
                if position > 0 {
                    output.push(' ');
                }
                self.format_inline_or_node(tree, *child, depth + 1, output);
            } else if position == prefix_len {
                // The distinguished argument: stays on the head line
                // when it fits, breaks to distinguished_column (+4) when
                // it does not.
                let col = Self::last_line_width(output).saturating_add(1);
                let inline =
                    if compact_distinguished_list && tree.node(*child).kind == NodeKind::List {
                        self.compact_form(tree, *child, col)
                    } else {
                        self.compact_node(tree, *child, col)
                    };
                if let Some(inline) = inline {
                    output.push(' ');
                    output.push_str(&inline);
                } else {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        distinguished_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
            } else {
                self.break_between_nodes(
                    tree,
                    node.children[position - 1],
                    *child,
                    body_column,
                    output,
                );
                self.format_node(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_condition_case(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let base_column = Self::last_line_width(output);
        let handler_column = self.add_indent(base_column);
        let body_column = self.add_indent(handler_column);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                2 => {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        body_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
                _ => {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        handler_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_custom_variable(
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

        for (position, child) in node.children.iter().take(4).enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        body_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        for pair in node.children.get(4..).unwrap_or_default().chunks(2) {
            Self::break_to_column(body_column, output);
            self.format_inline_or_node(tree, pair[0], depth + 1, output);
            if let Some(value) = pair.get(1) {
                output.push(' ');
                self.format_inline_or_node(tree, *value, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_elisp_if(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let base_column = Self::last_line_width(output);
        let body_column = self.add_indent(base_column);
        let then_column = self.add_indent(body_column);
        output.push(delimiter.open());

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                2 => {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        then_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
                _ => {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        body_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    /// `if` in Common Lisp: the test shares the head line and all branches
    /// (then, else) align at the same distinguished column — two indent
    /// steps from the form's opening delimiter.
    pub(in crate::sexpr::formatter) fn format_if_aligned(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let base_column = Self::last_line_width(output);
        let body_column = self.add_indent(base_column);
        let branch_column = self.add_indent(body_column);
        output.push(delimiter.open());
        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => self.format_node(tree, *child, depth + 1, output),
                1 => {
                    output.push(' ');
                    self.format_inline_or_node(tree, *child, depth + 1, output);
                }
                _ => {
                    self.break_between_nodes(
                        tree,
                        node.children[position - 1],
                        *child,
                        branch_column,
                        output,
                    );
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }
        output.push(delimiter.close());
    }

    pub(in crate::sexpr::formatter) fn format_head_body(
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
            if position == 0 {
                self.format_node(tree, *child, depth + 1, output);
            } else {
                self.break_between_nodes(
                    tree,
                    node.children[position - 1],
                    *child,
                    body_column,
                    output,
                );
                self.format_node(tree, *child, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }
}
