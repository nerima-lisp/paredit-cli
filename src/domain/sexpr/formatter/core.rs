use super::styles::ListStyle;
use super::{Formatter, MAX_INLINE_WIDTH};
use crate::domain::sexpr::tree::{Node, NodeKind, SyntaxTree};
use crate::domain::sexpr::types::Delimiter;
use crate::domain::sexpr::types::NodeId;

const MAX_RECURSIVE_FORMAT_DEPTH: usize = 256;

/// One planned unit of top-level output: either a form (with the comments that
/// attach to it) or a run of standalone comments with no following form.
enum TopLevelItem {
    Form {
        node_id: NodeId,
        /// The last root node that belongs to this logical form. Metadata
        /// descriptors and their target are separate parser nodes.
        end_node_id: NodeId,
        /// Own-line comments emitted immediately above the form.
        leading: Vec<usize>,
        /// A comment trailing the form on the same source line, if any.
        trailing: Option<usize>,
        /// Render the form's original source verbatim to preserve interior
        /// comments rather than reformatting it.
        verbatim: bool,
    },
    Comments(Vec<usize>),
}

impl Formatter {
    #[must_use]
    pub fn new(indent: usize) -> Self {
        Self {
            indent: indent.min(MAX_INLINE_WIDTH),
        }
    }

    #[must_use]
    pub fn format(&self, tree: &SyntaxTree) -> String {
        let items = self.plan_top_level(tree);
        if items.is_empty() {
            return String::new();
        }
        let mut output = String::new();
        for (position, item) in items.iter().enumerate() {
            if position > 0 {
                // Top-level items are separated by a single blank line, matching
                // the canonical style for comment-free documents.
                output.push_str("\n\n");
            }
            self.render_top_level_item(tree, item, &mut output);
        }
        output.push('\n');
        output
    }

    /// Groups root-level forms with the comments that surround them so the
    /// formatter can re-emit every comment without dropping or reordering it.
    ///
    /// Comments that sit *inside* a form force that form to render verbatim,
    /// which keeps interior comments exactly where the author placed them while
    /// still canonicalising comment-free forms.
    fn plan_top_level(&self, tree: &SyntaxTree) -> Vec<TopLevelItem> {
        let comments = &tree.comments;
        let mut order: Vec<usize> = (0..comments.len()).collect();
        order.sort_by_key(|&index| comments[index].span.start().get());

        let root_children = tree.root_children();
        let mut node_index = 0usize;
        let mut cursor = 0usize;
        let mut items: Vec<TopLevelItem> = Vec::new();

        while node_index < root_children.len() {
            let node_id = root_children[node_index];
            let start = tree.node(node_id).span.start().get();
            let mut end_node_id = node_id;

            while tree
                .node(end_node_id)
                .reader_prefixes
                .iter()
                .any(|prefix| matches!(prefix, crate::domain::sexpr::tree::ReaderPrefix::Metadata))
                && node_index + 1 < root_children.len()
            {
                node_index += 1;
                end_node_id = root_children[node_index];
            }

            let end = tree.node(end_node_id).span.end().get();
            let mut leading = Vec::new();
            while cursor < order.len() && comments[order[cursor]].span.start().get() < start {
                leading.push(order[cursor]);
                cursor += 1;
            }

            let mut verbatim = node_id != end_node_id;
            while cursor < order.len() && comments[order[cursor]].span.start().get() < end {
                verbatim = true;
                cursor += 1;
            }

            let item_index = items.len();
            items.push(TopLevelItem::Form {
                node_id,
                end_node_id,
                leading,
                trailing: None,
                verbatim,
            });

            if cursor < order.len() {
                let comment = &comments[order[cursor]];
                let comment_start = comment.span.start().get();
                let same_line =
                    comment_start >= end && !tree.source[end..comment_start].contains('\n');
                if !comment.own_line && same_line {
                    if let TopLevelItem::Form { trailing, .. } = &mut items[item_index] {
                        *trailing = Some(order[cursor]);
                    }
                    cursor += 1;
                }
            }

            node_index += 1;
        }

        if cursor < order.len() {
            items.push(TopLevelItem::Comments(order[cursor..].to_vec()));
        }

        items
    }

    fn render_top_level_item(&self, tree: &SyntaxTree, item: &TopLevelItem, output: &mut String) {
        let comments = &tree.comments;
        match item {
            TopLevelItem::Form {
                node_id,
                end_node_id,
                leading,
                trailing,
                verbatim,
            } => {
                for &comment in leading {
                    output.push_str(comments[comment].text.trim_end());
                    output.push('\n');
                }
                if *verbatim {
                    let start = tree.node(*node_id).span.start().get();
                    let end = tree.node(*end_node_id).span.end().get();
                    output.push_str(&tree.source[start..end]);
                } else {
                    self.format_node(tree, *node_id, 0, output);
                }
                if let Some(comment) = trailing {
                    output.push(' ');
                    output.push_str(comments[*comment].text.trim_end());
                }
            }
            TopLevelItem::Comments(indices) => {
                for (position, &comment) in indices.iter().enumerate() {
                    if position > 0 {
                        output.push('\n');
                    }
                    output.push_str(comments[comment].text.trim_end());
                }
            }
        }
    }

    pub(super) fn format_node(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if depth >= MAX_RECURSIVE_FORMAT_DEPTH {
            output.push_str(node.span.slice(&tree.source));
            return;
        }
        match node.kind {
            NodeKind::Root => (),
            NodeKind::Atom => {
                output.push_str(node.span.slice(&tree.source));
            }
            NodeKind::List if node.children.is_empty() => {
                if self.is_opaque_reader_form(node) {
                    output.push_str(node.span.slice(&tree.source));
                    return;
                }
                self.write_reader_prefixes(tree, node, output);
                let delimiter = self.list_delimiter(node);
                output.push(delimiter.open());
                output.push(delimiter.close());
            }
            NodeKind::List => {
                if let Some(inline) = self.inline_list(tree, node_id) {
                    output.push_str(&inline);
                    return;
                }
                if self.is_opaque_reader_form(node) {
                    output.push_str(node.span.slice(&tree.source));
                    return;
                }
                self.write_reader_prefixes(tree, node, output);
                if let Some(head) = self.head_text(tree, node_id) {
                    match self.style_for_head(head) {
                        ListStyle::Definition => {
                            self.format_definition(tree, node_id, depth, output);
                        }
                        ListStyle::SystemDefinition => {
                            self.format_system_definition(tree, node_id, depth, output);
                        }
                        ListStyle::Defmethod => {
                            self.format_defmethod(tree, node_id, depth, output);
                        }
                        ListStyle::DefinitionNameBody => {
                            self.format_prefix_body(tree, node_id, depth, 1, output);
                        }
                        ListStyle::Lambda => {
                            self.format_prefix_body(tree, node_id, depth, 1, output);
                        }
                        ListStyle::NamedLambda => {
                            self.format_prefix_body(tree, node_id, depth, 2, output);
                        }
                        ListStyle::Binding => {
                            self.format_binding_form(tree, node_id, depth, output);
                        }
                        ListStyle::LocalFunctions => {
                            self.format_local_callable_form(tree, node_id, depth, head, output);
                        }
                        ListStyle::OneArgumentBody => {
                            self.format_prefix_body(tree, node_id, depth, 1, output);
                        }
                        ListStyle::TwoArgumentBody => {
                            self.format_prefix_body(tree, node_id, depth, 2, output);
                        }
                        ListStyle::ClauseForm => {
                            self.format_clause_form(tree, node_id, depth, output);
                        }
                        ListStyle::CondClauses => {
                            self.format_cond_clauses(tree, node_id, depth, output);
                        }
                        ListStyle::CaseClauses => {
                            self.format_case_clauses(tree, node_id, depth, output);
                        }
                        ListStyle::Do => {
                            self.format_do_form(tree, node_id, depth, head, output);
                        }
                        ListStyle::Prog => {
                            self.format_prog_form(tree, node_id, depth, head, output);
                        }
                        ListStyle::Declaration => {
                            self.format_declaration_form(tree, node_id, depth, head, output);
                        }
                        ListStyle::PairAssignment => {
                            self.format_pair_assignment_form(tree, node_id, depth, head, output);
                        }
                        ListStyle::Loop => {
                            self.format_loop_form(tree, node_id, depth, head, output);
                        }
                        ListStyle::HeadBody => {
                            self.format_head_body(tree, node_id, depth, output);
                        }
                        ListStyle::If => {
                            self.format_prefix_body(tree, node_id, depth, 2, output);
                        }
                        ListStyle::General => {
                            self.format_general_list(tree, node_id, depth, output);
                        }
                    }
                } else {
                    self.format_general_list(tree, node_id, depth, output);
                }
            }
        }
    }

    pub(super) fn inline_list(&self, tree: &SyntaxTree, node_id: NodeId) -> Option<String> {
        let head = self.head_text(tree, node_id);
        if head.is_some_and(|head| self.style_for_head(head) != ListStyle::General) {
            return None;
        }
        self.compact_node(tree, node_id)
    }

    pub(super) fn compact_node(&self, tree: &SyntaxTree, node_id: NodeId) -> Option<String> {
        fn push_bounded(output: &mut String, value: &str) -> Option<()> {
            let new_len = output.len().checked_add(value.len())?;
            if new_len > MAX_INLINE_WIDTH {
                return None;
            }
            output.push_str(value);
            Some(())
        }

        fn push_char_bounded(output: &mut String, value: char) -> Option<()> {
            let new_len = output.len().checked_add(value.len_utf8())?;
            if new_len > MAX_INLINE_WIDTH {
                return None;
            }
            output.push(value);
            Some(())
        }

        enum Action {
            Node(NodeId),
            Separator,
            Close(char),
        }

        let mut output = String::new();
        let mut actions = vec![Action::Node(node_id)];

        while let Some(action) = actions.pop() {
            match action {
                Action::Separator => push_char_bounded(&mut output, ' ')?,
                Action::Close(delimiter) => push_char_bounded(&mut output, delimiter)?,
                Action::Node(current_id) => {
                    let node = tree.node(current_id);
                    match node.kind {
                        NodeKind::Root => return None,
                        NodeKind::Atom => {
                            push_bounded(&mut output, node.span.slice(&tree.source))?;
                        }
                        NodeKind::List if self.is_opaque_reader_form(node) => {
                            push_bounded(&mut output, node.span.slice(&tree.source))?;
                        }
                        NodeKind::List => {
                            if self
                                .head_text(tree, current_id)
                                .is_some_and(|head| self.style_for_head(head) != ListStyle::General)
                            {
                                return None;
                            }

                            for span in &node.reader_prefix_spans {
                                push_bounded(&mut output, span.slice(&tree.source))?;
                            }
                            let delimiter = self.list_delimiter(node);
                            push_char_bounded(&mut output, delimiter.open())?;
                            actions.push(Action::Close(delimiter.close()));
                            for (position, child) in node.children.iter().enumerate().rev() {
                                actions.push(Action::Node(*child));
                                if position > 0 {
                                    actions.push(Action::Separator);
                                }
                            }
                        }
                    }
                }
            }
        }

        Some(output)
    }

    pub(super) fn format_inline_or_node(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        if let Some(inline) = self.compact_node(tree, node_id) {
            output.push_str(&inline);
        } else {
            self.format_node(tree, node_id, depth, output);
        }
    }

    pub(super) fn head_text<'a>(&self, tree: &'a SyntaxTree, node_id: NodeId) -> Option<&'a str> {
        let node = tree.node(node_id);
        let first = *node.children.first()?;
        let first = tree.node(first);
        (first.kind == NodeKind::Atom && first.reader_prefixes.is_empty())
            .then(|| first.span.slice(&tree.source))
    }

    pub(super) fn list_delimiter(&self, node: &Node) -> Delimiter {
        node.delimiter.unwrap_or_else(|| {
            debug_assert!(false, "list node missing delimiter during formatting");
            Delimiter::Paren
        })
    }

    pub(super) fn write_reader_prefixes(
        &self,
        tree: &SyntaxTree,
        node: &Node,
        output: &mut String,
    ) {
        for span in &node.reader_prefix_spans {
            output.push_str(span.slice(&tree.source));
        }
    }

    pub(super) fn is_opaque_reader_form(&self, node: &Node) -> bool {
        node.opaque_reader_form
            || node
                .reader_prefixes
                .iter()
                .any(|prefix| prefix.is_opaque_reader_form())
    }

    pub(super) fn indent(&self, depth: usize) -> String {
        " ".repeat(self.indentation_width(depth))
    }

    pub(super) fn continuation_column(&self, depth: usize, offset: usize) -> usize {
        self.indentation_width(depth).saturating_add(offset)
    }

    pub(super) fn add_indent(&self, column: usize) -> usize {
        column.saturating_add(self.indent)
    }

    fn indentation_width(&self, depth: usize) -> usize {
        depth.saturating_mul(self.indent)
    }
}
