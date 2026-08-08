use unicode_width::UnicodeWidthStr;

use crate::sexpr::formatter::Formatter;
use crate::sexpr::tree::{NodeKind, SyntaxTree};
use crate::sexpr::types::{Delimiter, NodeId};

impl Formatter {
    pub(in crate::sexpr::formatter) fn format_general_list(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let base_column = Self::last_line_width(output);
        output.push(delimiter.open());

        // When the first argument shares the head's line, subsequent
        // arguments align under it (Emacs `lisp-indent-function` default).
        // Otherwise every argument lands one column past the opening
        // delimiter — the same fallback `reindent.rs` uses.
        let mut first_arg_column: Option<usize> = None;
        let mut head_line_count: Option<usize> = None;

        for (position, child) in node.children.iter().enumerate() {
            match position {
                0 => {
                    self.format_node(tree, *child, depth + 1, output);
                    head_line_count = Some(output.lines().count());
                }
                1 => {
                    // If the first argument fits inline on the head line,
                    // align subsequent siblings under it. Otherwise break
                    // to one column past the opening delimiter.
                    let child_start = Self::last_line_width(output).saturating_add(1);
                    // When this argument is also the last child, the list's
                    // own closing delimiter lands right after it on the same
                    // line, so it must be charged against the budget too —
                    // `compact_node` only measures the argument's own text.
                    let is_last_child = position + 1 == node.children.len();
                    let max_width = self.effective_max_width(tree, node_id);
                    let inline = self.compact_node(tree, *child, child_start).filter(|inline| {
                        !is_last_child
                            || child_start.saturating_add(UnicodeWidthStr::width(inline.as_str()))
                                < max_width
                    });
                    if let Some(inline) = inline {
                        output.push(' ');
                        let col = Self::last_line_width(output);
                        output.push_str(&inline);
                        if head_line_count.is_some_and(|hl| output.lines().count() == hl) {
                            first_arg_column = Some(col);
                        }
                    } else {
                        let element_column = base_column.saturating_add(1);
                        Self::break_to_column(element_column, output);
                        self.format_node(tree, *child, depth + 1, output);
                    }
                }
                _ => {
                    let element_column = first_arg_column.unwrap_or(base_column.saturating_add(1));
                    Self::break_to_column(element_column, output);
                    self.format_node(tree, *child, depth + 1, output);
                }
            }
        }

        output.push(delimiter.close());
    }

    /// Lays out a `let`-style bindings list or a `do`/`prog` var-list: one
    /// entry per line past the first, every entry sharing the column the
    /// first one lands on.
    ///
    /// `align_values` is `format.align-clause-values` (FR-013), always
    /// passed explicitly by the caller rather than read from `self` here:
    /// `format_binding_form` (a `let`/`let*`-style binding list — the shape
    /// the option is scoped to) passes `self.align_clause_values`, while
    /// `format_clause_sequence_form` (`do`/`prog`'s var-list, which allows a
    /// third "step" element a two-column name/value layout does not fit)
    /// always passes `false` regardless of the option — see
    /// `Formatter::align_target_columns_for_bindings`'s doc comment for why
    /// that shape is out of scope rather than force-fit.
    pub(in crate::sexpr::formatter) fn format_sequence_list(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        align_values: bool,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        if node.kind != NodeKind::List
            || node.children.is_empty()
            || Formatter::carries_reader_prefix(node)
        {
            self.format_inline_or_node(tree, node_id, depth, output);
            return;
        }

        let delimiter = self.list_delimiter(node);
        let entry_column = Self::last_line_width(output).saturating_add(1);
        output.push(delimiter.open());
        if delimiter == Delimiter::Bracket && node.children.len() % 2 == 0 {
            let columns =
                align_values.then(|| self.align_target_columns_for_pairs(tree, &node.children));
            for (position, pair) in node.children.chunks_exact(2).enumerate() {
                if position > 0 {
                    Self::break_to_column(entry_column, output);
                }
                if let Some(Some((name, column))) =
                    columns.as_ref().map(|columns| &columns[position])
                {
                    Self::write_aligned_value(name, *column, output);
                    self.format_inline_or_node(tree, pair[1], depth + 1, output);
                    continue;
                }
                self.format_inline_or_node(tree, pair[0], depth + 1, output);
                output.push(' ');
                self.format_inline_or_node(tree, pair[1], depth + 1, output);
            }
            output.push(delimiter.close());
            return;
        }

        let columns =
            align_values.then(|| self.align_target_columns_for_bindings(tree, &node.children));
        for (position, child) in node.children.iter().enumerate() {
            if position > 0 {
                Self::break_to_column(entry_column, output);
            }
            if let Some(Some((name, column))) = columns.as_ref().map(|columns| &columns[position]) {
                let binding = tree.node(*child);
                let inner_delimiter = self.list_delimiter(binding);
                output.push(inner_delimiter.open());
                Self::write_aligned_value(name, *column, output);
                self.format_inline_or_node(tree, binding.children[1], depth + 1, output);
                output.push(inner_delimiter.close());
                continue;
            }
            self.format_inline_or_node(tree, *child, depth + 1, output);
        }
        output.push(delimiter.close());
    }

    /// Writes `name` followed by however many spaces pad it out to `column`
    /// (at least one), the shared tail of both `format_sequence_list`
    /// alignment branches.
    fn write_aligned_value(name: &str, column: usize, output: &mut String) {
        output.push_str(name);
        let padding = column.saturating_sub(UnicodeWidthStr::width(name)).max(1);
        output.push_str(&" ".repeat(padding));
    }

    /// Per-position `(name text, target column)` for a `let`-style binding
    /// list — each `child` a two-element list `(name value)` — under
    /// `format.align-clause-values`. `None` at a position that is not a
    /// two-element list, or whose name does not compact to one line: either
    /// way, that position renders exactly as it would with the option off,
    /// and breaks the run the same way a form without a trailing comment
    /// breaks a run in [`Self::trailing_comment_columns`] (FR-005) — the
    /// same "adjacent and same-shaped, or the run ends" rule, applied here to
    /// binding values instead of trailing comments.
    ///
    /// A bare-symbol binding (`(let ((x 1) y) ...)`), a destructuring name
    /// that does not fit on one line, and a `do`/`prog`-style three-element
    /// `(var init step)` clause (which `format_sequence_list` never even
    /// calls this for — see its own doc comment) all fall outside this
    /// shape, by design: forcing an unrelated column onto them would be
    /// aligning things that do not mean the same thing.
    fn align_target_columns_for_bindings(
        &self,
        tree: &SyntaxTree,
        children: &[NodeId],
    ) -> Vec<Option<(String, usize)>> {
        let names: Vec<Option<String>> = children
            .iter()
            .map(|&child| self.alignable_binding_name(tree, child))
            .collect();
        Self::pair_names_with_columns(names)
    }

    /// The same computation as [`Self::align_target_columns_for_bindings`],
    /// for a Clojure-style flat binding vector (`[name value name value …]`)
    /// instead of a Common-Lisp-style list of `(name value)` sublists.
    fn align_target_columns_for_pairs(
        &self,
        tree: &SyntaxTree,
        children: &[NodeId],
    ) -> Vec<Option<(String, usize)>> {
        // Start column `0`: this measures a name's own text to derive an
        // alignment column, and is not a decision about whether some line
        // fits.
        let names: Vec<Option<String>> = children
            .chunks_exact(2)
            .map(|pair| self.compact_node(tree, pair[0], 0))
            .collect();
        Self::pair_names_with_columns(names)
    }

    /// A binding's name, compacted to one line, when `node_id` is a
    /// two-element list `(name value)` — the only shape alignment applies to.
    fn alignable_binding_name(&self, tree: &SyntaxTree, node_id: NodeId) -> Option<String> {
        let node = tree.node(node_id);
        // A prefixed entry is excluded for the same reason a bare symbol is —
        // it is not the `(name value)` shape — and, additionally, because the
        // aligned branch writes the entry's opening delimiter itself and would
        // drop the prefix; see `Formatter::carries_reader_prefix`.
        if node.kind != NodeKind::List
            || node.children.len() != 2
            || Formatter::carries_reader_prefix(node)
        {
            return None;
        }
        // Start column `0`, for the same reason as
        // `align_target_columns_for_pairs`.
        self.compact_node(tree, node.children[0], 0)
    }

    /// Zips `names` (one entry per binding, `None` where alignment cannot
    /// apply) with the column [`Self::align_target_columns`] assigns each.
    fn pair_names_with_columns(names: Vec<Option<String>>) -> Vec<Option<(String, usize)>> {
        let columns = Self::align_target_columns(&names);
        names
            .into_iter()
            .zip(columns)
            .map(|(name, column)| name.zip(column))
            .collect()
    }

    /// The value-alignment target column for each position in `names`: a
    /// maximal run of consecutive `Some` entries shares one column, one past
    /// the widest name in that run (in display columns, via
    /// [`UnicodeWidthStr::width`]) — the same run-detection principle
    /// [`Formatter::trailing_comment_columns`] (FR-005) applies to trailing
    /// comments, applied here to binding values instead. A run of exactly
    /// one resolves to that binding's own width plus one column, identical
    /// to the unaligned single-space default, so a solitary alignable
    /// binding is never visibly changed by turning this option on.
    fn align_target_columns(names: &[Option<String>]) -> Vec<Option<usize>> {
        let mut columns = vec![None; names.len()];
        let mut index = 0;
        while index < names.len() {
            if names[index].is_none() {
                index += 1;
                continue;
            }
            let run_start = index;
            let mut run_end = index;
            while run_end + 1 < names.len() && names[run_end + 1].is_some() {
                run_end += 1;
            }
            let widest = (run_start..=run_end)
                .map(|member| UnicodeWidthStr::width(names[member].as_deref().unwrap_or("")))
                .max()
                .unwrap_or(0);
            let target = widest.saturating_add(1);
            for column in &mut columns[run_start..=run_end] {
                *column = Some(target);
            }
            index = run_end + 1;
        }
        columns
    }
}
