use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::styles::ListStyle;
use super::{Formatter, MAX_INLINE_WIDTH};
use crate::dialect::Dialect;
use crate::sexpr::tree::{Node, NodeKind, SyntaxTree};
use crate::sexpr::types::Delimiter;
use crate::sexpr::types::NodeId;

const MAX_RECURSIVE_FORMAT_DEPTH: usize = 256;

/// The text accumulated so far, plus its running East-Asian-Width–aware
/// display width — tracked separately from `text.len()`, which is a byte
/// count and not a column count for anything outside ASCII.
///
/// Shared between [`Formatter::compact_node`] here and
/// [`Formatter::compact_form`] (in `formatter/lists/definitions.rs`) so both
/// build their output against a running width instead of re-scanning the
/// whole accumulated string every time they need to check it fits.
pub(super) struct Bounded {
    text: String,
    width: usize,
}

impl Bounded {
    pub(super) const fn new() -> Self {
        Self {
            text: String::new(),
            width: 0,
        }
    }

    /// Starts already charged `width` columns against the budget — for a
    /// caller that writes some text (e.g. reader-prefix spans) outside this
    /// accumulator but still wants it counted toward `max_width`.
    pub(super) const fn with_initial_width(width: usize) -> Self {
        Self {
            text: String::new(),
            width,
        }
    }

    pub(super) fn push_str(&mut self, value: &str, max_width: usize) -> Option<()> {
        let new_width = self.width.checked_add(UnicodeWidthStr::width(value))?;
        if new_width > max_width {
            return None;
        }
        self.text.push_str(value);
        self.width = new_width;
        Some(())
    }

    pub(super) fn push_char(&mut self, value: char, max_width: usize) -> Option<()> {
        let new_width = self
            .width
            .checked_add(UnicodeWidthChar::width(value).unwrap_or(0))?;
        if new_width > max_width {
            return None;
        }
        self.text.push(value);
        self.width = new_width;
        Some(())
    }

    pub(super) fn into_text(self) -> String {
        self.text
    }
}

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
    /// Builds a formatter that lays every dialect out with the Common Lisp
    /// operator table.
    ///
    /// Prefer [`Formatter::with_dialect`] when the dialect is known: Clojure
    /// forms only get their own layouts through that constructor.
    #[must_use]
    pub fn new(indent: usize) -> Self {
        Self::with_dialect(indent, Dialect::Unknown)
    }

    /// Builds a formatter that lays lists out with `dialect`'s operator table.
    #[must_use]
    pub fn with_dialect(indent: usize, dialect: Dialect) -> Self {
        Self {
            indent: indent.min(MAX_INLINE_WIDTH),
            dialect,
            max_width: MAX_INLINE_WIDTH,
            reindent_block_comments: false,
        }
    }

    /// Overrides the inline-width budget [`Formatter::with_dialect`] set to
    /// [`MAX_INLINE_WIDTH`].
    ///
    /// Re-clamps `indent` to the new width: an indent wider than the line
    /// budget could never fit an inline form regardless, and every existing
    /// caller of `with_dialect` already accepts silent clamping to the
    /// compiled-in default for the same reason.
    #[must_use]
    pub fn with_max_width(mut self, max_width: usize) -> Self {
        self.max_width = max_width;
        self.indent = self.indent.min(max_width);
        self
    }

    /// Enables realigning `#|...|#` block comment lines to their nesting
    /// depth. `false` by default: this changes what a block comment's
    /// interior looks like, not merely where a line breaks, so a caller opts
    /// in rather than finding their comments rewritten by default.
    #[must_use]
    pub const fn with_reindent_block_comments(mut self, reindent: bool) -> Self {
        self.reindent_block_comments = reindent;
        self
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
                .any(|prefix| matches!(prefix, crate::sexpr::tree::ReaderPrefix::Metadata))
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
                    output.push_str(&self.render_comment_text(&comments[comment].text, 0));
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
                    output.push_str(&self.render_comment_text(&comments[*comment].text, 0));
                }
            }
            TopLevelItem::Comments(indices) => {
                for (position, &comment) in indices.iter().enumerate() {
                    if position > 0 {
                        output.push('\n');
                    }
                    output.push_str(&self.render_comment_text(&comments[comment].text, 0));
                }
            }
        }
    }

    /// Trims a comment's trailing whitespace and, when enabled, realigns a
    /// `#|...|#` block comment's interior lines to `depth`.
    ///
    /// Every call site in [`Self::render_top_level_item`] is a standalone or
    /// trailing comment between top-level forms, so `depth` is always `0`
    /// today; the depth parameter still exists because the transformation
    /// itself has nothing top-level-specific about it.
    fn render_comment_text(&self, text: &str, depth: usize) -> String {
        let trimmed = text.trim_end();
        if self.reindent_block_comments {
            self.reindent_block_comment(trimmed, depth)
        } else {
            trimmed.to_owned()
        }
    }

    /// Realigns a `#|...|#` block comment's continuation lines to `depth`,
    /// preserving each line's indentation *relative to* the others.
    ///
    /// The comment's own nested `#|...|#` markers need no special handling:
    /// the parser already matched them when it produced `text`, so this only
    /// ever adjusts a leading-whitespace prefix, never comment content.
    ///
    /// A line whose transformation this function is not sure of — narrower
    /// than every other continuation line, which `base` being their minimum
    /// should make impossible — is left exactly as written rather than
    /// guessed at.
    ///
    /// Each line's leading-whitespace amount is measured in display width
    /// (via [`UnicodeWidthStr::width`]), not byte length, so a tab and a
    /// space are weighted consistently with the rest of this file's width
    /// accounting; a tab's width is its raw `unicode-width` value (1 column),
    /// not an expansion to the next tab stop.
    pub(super) fn reindent_block_comment(&self, text: &str, depth: usize) -> String {
        if !text.starts_with("#|") {
            return text.to_owned();
        }
        let mut lines = text.split('\n');
        let Some(first) = lines.next() else {
            return text.to_owned();
        };
        let rest: Vec<&str> = lines.collect();
        if rest.is_empty() {
            // A single-line block comment has nothing to realign.
            return text.to_owned();
        }

        let Some(base) = rest
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| UnicodeWidthStr::width(&line[..line.len() - line.trim_start().len()]))
            .min()
        else {
            // Every continuation line is blank; there is nothing to measure
            // an indentation against.
            return text.to_owned();
        };

        let new_indent = self.indent(depth);
        let mut result = String::with_capacity(text.len());
        result.push_str(first);
        for line in rest {
            result.push('\n');
            if line.trim().is_empty() {
                // Blank interior lines are left exactly as written.
                result.push_str(line);
                continue;
            }
            let leading_bytes = line.len() - line.trim_start().len();
            let leading = UnicodeWidthStr::width(&line[..leading_bytes]);
            if leading < base {
                result.push_str(line);
                continue;
            }
            result.push_str(&new_indent);
            result.push_str(&" ".repeat(leading - base));
            result.push_str(line.trim_start());
        }
        result
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
                // A reader conditional is keyed off its prefix, not its head:
                // the head of `#?(:clj a :cljs b)` is the feature keyword.
                if self.dialect == Dialect::Clojure && Self::is_clojure_reader_conditional(node) {
                    self.format_clojure_reader_conditional(tree, node_id, depth, output);
                    return;
                }
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
                        ListStyle::ClojureDefinition => {
                            self.format_clojure_definition(tree, node_id, depth, output);
                        }
                        ListStyle::ClojureFunction => {
                            self.format_clojure_function(tree, node_id, depth, output);
                        }
                        ListStyle::ClojurePrefixBody(prefix_len) => {
                            self.format_clojure_prefix_body(
                                tree, node_id, depth, prefix_len, output,
                            );
                        }
                        ListStyle::ClojureThreading(prefix_len) => {
                            self.format_clojure_threading(
                                tree, node_id, depth, head, prefix_len, output,
                            );
                        }
                        ListStyle::ClojurePairClauses(prefix_len) => {
                            self.format_clojure_pair_clauses(
                                tree, node_id, depth, prefix_len, output,
                            );
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
        enum Action {
            Node(NodeId),
            Separator,
            Close(char),
        }

        let max_width = self.max_width;
        let mut output = Bounded::new();
        let mut actions = vec![Action::Node(node_id)];

        while let Some(action) = actions.pop() {
            match action {
                Action::Separator => output.push_char(' ', max_width)?,
                Action::Close(delimiter) => output.push_char(delimiter, max_width)?,
                Action::Node(current_id) => {
                    let node = tree.node(current_id);
                    match node.kind {
                        NodeKind::Root => return None,
                        NodeKind::Atom => {
                            output.push_str(node.span.slice(&tree.source), max_width)?;
                        }
                        NodeKind::List if self.is_opaque_reader_form(node) => {
                            output.push_str(node.span.slice(&tree.source), max_width)?;
                        }
                        NodeKind::List => {
                            if self
                                .head_text(tree, current_id)
                                .is_some_and(|head| self.style_for_head(head) != ListStyle::General)
                            {
                                return None;
                            }

                            for span in &node.reader_prefix_spans {
                                output.push_str(span.slice(&tree.source), max_width)?;
                            }
                            let delimiter = self.list_delimiter(node);
                            output.push_char(delimiter.open(), max_width)?;
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

        Some(output.into_text())
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

    pub(super) const fn continuation_column(&self, depth: usize, offset: usize) -> usize {
        self.indentation_width(depth).saturating_add(offset)
    }

    pub(super) const fn add_indent(&self, column: usize) -> usize {
        column.saturating_add(self.indent)
    }

    const fn indentation_width(&self, depth: usize) -> usize {
        depth.saturating_mul(self.indent)
    }
}
