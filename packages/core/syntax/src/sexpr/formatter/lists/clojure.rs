use crate::sexpr::formatter::Formatter;
use crate::sexpr::tree::{Node, NodeKind, ReaderPrefix, SyntaxTree};
use crate::sexpr::types::{Delimiter, NodeId};

impl Formatter {
    /// Formats `defn`, `defn-`, and `defmacro`.
    ///
    /// `(defn name [params]` keeps the parameter vector on the head line. When
    /// something else follows the name -- a docstring, an attribute map, or the
    /// `([params] body)` clauses of a multi-arity form -- only the name stays
    /// there and every following child lands on its own line.
    pub(in crate::sexpr::formatter) fn format_clojure_definition(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let children = Self::logical_children(tree, node_id);
        let prefix_len = if Self::is_parameter_vector(tree, &children, 2) {
            2
        } else {
            1
        };
        self.format_clojure_prefix_body(tree, node_id, depth, prefix_len, output);
    }

    /// Formats `fn`, whose parameter vector may be preceded by a self-reference
    /// name and may be replaced by several arity clauses.
    pub(in crate::sexpr::formatter) fn format_clojure_function(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let children = Self::logical_children(tree, node_id);
        let prefix_len = if Self::is_parameter_vector(tree, &children, 1) {
            // `(fn [params] body)`.
            1
        } else if Self::logical_child(tree, &children, 1)
            .is_some_and(|child| child.kind == NodeKind::Atom)
        {
            // `(fn name [params] body)`.
            2
        } else {
            // `(fn ([x] ...) ([x y] ...))`: every arity clause gets its own line.
            0
        };
        self.format_clojure_prefix_body(tree, node_id, depth, prefix_len, output);
    }

    /// Renders a Clojure form that keeps its head plus `prefix_len` children on
    /// the head line and indents every remaining child one per line.
    ///
    /// Unlike the Common Lisp prefix-body styles, a form whose layout would
    /// leave a single body child collapses back onto one line when it fits the
    /// width budget. Breaking gains nothing there -- there are no siblings to
    /// line up -- and `(def data #{1 2 3})`, `(defmulti encode :type)`, and
    /// `(defn- helper [x y] (+ x y))` are how Clojure actually spells those
    /// forms.
    pub(in crate::sexpr::formatter) fn format_clojure_prefix_body(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        prefix_len: usize,
        output: &mut String,
    ) {
        let children = Self::logical_children(tree, node_id);
        if let Some(inline) =
            self.compact_single_body_form(tree, node_id, children.len(), prefix_len)
        {
            output.push_str(&inline);
            return;
        }

        let delimiter = self.list_delimiter(tree.node(node_id));
        output.push(delimiter.open());

        for (position, child) in children.iter().enumerate() {
            if position <= prefix_len {
                if position > 0 {
                    output.push(' ');
                }
            } else {
                output.push('\n');
                output.push_str(&self.indent(depth + 1));
            }
            self.format_logical_child(tree, child, depth + 1, output);
        }

        output.push(delimiter.close());
    }

    /// Formats a threading macro such as `->`, `->>`, or `as->`.
    ///
    /// The head and `prefix_len` children stay on the head line and each
    /// remaining step goes on its own line, aligned under the threaded value the
    /// way `cljfmt` aligns a call's arguments.
    pub(in crate::sexpr::formatter) fn format_clojure_threading(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        prefix_len: usize,
        output: &mut String,
    ) {
        let children = Self::logical_children(tree, node_id);
        if let Some(inline) =
            self.compact_single_body_form(tree, node_id, children.len(), prefix_len)
        {
            output.push_str(&inline);
            return;
        }

        let delimiter = self.list_delimiter(tree.node(node_id));
        let continuation_column = self.continuation_column(depth, head.len().saturating_add(2));
        output.push(delimiter.open());

        for (position, child) in children.iter().enumerate() {
            if position <= prefix_len {
                if position > 0 {
                    output.push(' ');
                }
            } else {
                output.push('\n');
                output.push_str(&" ".repeat(continuation_column));
            }
            self.format_logical_child(tree, child, depth + 1, output);
        }

        output.push(delimiter.close());
    }

    /// Formats a clause form such as `cond`, `case`, or `condp`, whose children
    /// after the first `prefix_len` are a flat run of test/result pairs rather
    /// than nested clause lists.
    ///
    /// Each pair stays together on one line. A trailing child without a partner
    /// -- `case`'s default expression -- gets a line of its own.
    pub(in crate::sexpr::formatter) fn format_clojure_pair_clauses(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        prefix_len: usize,
        output: &mut String,
    ) {
        let children = Self::logical_children(tree, node_id);
        let delimiter = self.list_delimiter(tree.node(node_id));
        let head_line_end = prefix_len.saturating_add(1).min(children.len());
        output.push(delimiter.open());

        for (position, child) in children[..head_line_end].iter().enumerate() {
            if position > 0 {
                output.push(' ');
            }
            self.format_logical_child(tree, child, depth + 1, output);
        }

        for pair in children[head_line_end..].chunks(2) {
            output.push('\n');
            output.push_str(&self.indent(depth + 1));
            self.format_logical_child(tree, pair[0], depth + 1, output);
            if let Some(result) = pair.get(1) {
                output.push(' ');
                self.format_logical_child(tree, result, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    /// Formats a reader conditional, whose children are a flat run of
    /// `feature form` pairs.
    ///
    /// This is keyed off the reader prefix rather than a head symbol, because
    /// the head of `#?(:clj a :cljs b)` is the feature keyword `:clj`. Without
    /// it the form would fall through to the general layout, which would put
    /// every feature and every form on a separate line.
    pub(in crate::sexpr::formatter) fn format_clojure_reader_conditional(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        // The `#?`/`#?@` prefix is already written, so continuation lines align
        // just inside the open delimiter that follows it.
        let prefix_width = node
            .reader_prefix_spans
            .iter()
            .map(|span| span.slice(&tree.source).len())
            .sum::<usize>();
        let continuation_column = self.continuation_column(depth, prefix_width.saturating_add(1));
        output.push(delimiter.open());

        for (position, pair) in node.children.chunks(2).enumerate() {
            if position > 0 {
                output.push('\n');
                output.push_str(&" ".repeat(continuation_column));
            }
            self.format_inline_or_node(tree, pair[0], depth + 1, output);
            if let Some(form) = pair.get(1) {
                output.push(' ');
                self.format_inline_or_node(tree, *form, depth + 1, output);
            }
        }

        output.push(delimiter.close());
    }

    /// Returns whether `node` is a `#?(...)` or `#?@(...)` reader conditional.
    pub(in crate::sexpr::formatter) fn is_clojure_reader_conditional(node: &Node) -> bool {
        node.reader_prefixes.iter().any(|prefix| {
            matches!(
                prefix,
                ReaderPrefix::ReaderConditional | ReaderPrefix::ReaderConditionalSplicing
            )
        })
    }

    /// Returns the one-line rendering of a form whose prefix-body layout would
    /// leave at most one child below the head line, when that line fits the
    /// width budget.
    ///
    /// A `prefix_len` of zero is excluded: that layout exists precisely to put
    /// the head alone on its line, so `(do x)` and `(comment x)` keep the shape
    /// their Common Lisp counterpart `(progn x)` has.
    fn compact_single_body_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        logical_child_count: usize,
        prefix_len: usize,
    ) -> Option<String> {
        (prefix_len > 0 && logical_child_count <= prefix_len.saturating_add(2))
            .then(|| self.compact_form(tree, node_id))
            .flatten()
    }

    /// Groups a list's children into logical children, gluing each `^meta`
    /// descriptor onto the form it decorates.
    ///
    /// The parser emits `^:private max-retries` as two sibling nodes, the same
    /// way [`Formatter::plan_top_level`] sees them at the root. A layout that
    /// counted raw children would tear every descriptor off its target and
    /// spell `(def ^:private ^:const max-retries 3)` across four lines.
    fn logical_children(tree: &SyntaxTree, node_id: NodeId) -> Vec<&[NodeId]> {
        let children = &tree.node(node_id).children;
        let mut groups = Vec::new();
        let mut start = 0usize;

        for (position, child) in children.iter().enumerate() {
            let carries_metadata = tree
                .node(*child)
                .reader_prefixes
                .iter()
                .any(|prefix| matches!(prefix, ReaderPrefix::Metadata));
            if !carries_metadata {
                groups.push(&children[start..=position]);
                start = position + 1;
            }
        }
        if start < children.len() {
            // A trailing descriptor with nothing left to decorate.
            groups.push(&children[start..]);
        }

        groups
    }

    /// Renders one logical child: its `^meta` descriptors and the form they
    /// decorate, separated by single spaces.
    fn format_logical_child(
        &self,
        tree: &SyntaxTree,
        children: &[NodeId],
        depth: usize,
        output: &mut String,
    ) {
        for (position, child) in children.iter().enumerate() {
            if position > 0 {
                output.push(' ');
            }
            self.format_node(tree, *child, depth, output);
        }
    }

    /// Returns the form a logical child decorates, i.e. its last node.
    fn logical_child<'a>(
        tree: &'a SyntaxTree,
        children: &[&[NodeId]],
        position: usize,
    ) -> Option<&'a Node> {
        children
            .get(position)
            .and_then(|child| child.last())
            .map(|child| tree.node(*child))
    }

    /// Returns whether the logical child at `position` is a `[...]` parameter
    /// vector.
    fn is_parameter_vector(tree: &SyntaxTree, children: &[&[NodeId]], position: usize) -> bool {
        Self::logical_child(tree, children, position).is_some_and(|child| {
            child.kind == NodeKind::List && child.delimiter == Some(Delimiter::Bracket)
        })
    }
}
