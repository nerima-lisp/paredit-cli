use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::styles::ListStyle;
use super::{Formatter, MAX_INLINE_WIDTH, NumericLiteralCase, ReaderPrefixStyle};
use crate::dialect::Dialect;
use crate::sexpr::quote_edit::canonical_quote_head;
use crate::sexpr::tree::{Node, NodeKind, SyntaxTree};
use crate::sexpr::types::Delimiter;
use crate::sexpr::types::NodeId;

const MAX_RECURSIVE_FORMAT_DEPTH: usize = 256;

/// A run of spaces long enough to fill a typical indent in one `push_str`,
/// used by [`Formatter::break_to_column`].
///
/// Filling by slices of this beats both obvious alternatives at every width,
/// because each `push_str` is a `memcpy` and nothing is allocated:
/// `" ".repeat(n)` allocates a fresh `String` per line break, and
/// `extend(repeat_n(' ', n))` goes through `String`'s `Extend<char>`, which is
/// a per-character UTF-8 encode branch. Measured at 2M line breaks (min of 5
/// interleaved rounds), the three cost, at columns 8 / 40 / 200:
/// `repeat` 37 / 56 / 61 ms, `extend` 10 / 39 / 188 ms, this 5 / 8 / 10 ms.
/// The column model makes wide indents common enough that `extend`'s blowup
/// past ~60 columns is reachable in ordinary formatting.
const SPACES: &str = "                                                                ";
// A zero-length run would make `break_to_column`'s fill loop spin forever.
const _: () = assert!(!SPACES.is_empty());

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
    /// Starts already charged `width` columns against the budget — for a
    /// caller whose rendering begins partway across a line, so the *fits on
    /// one line* decision is measured from the real column instead of giving
    /// a hugged form a fresh full-width allowance.
    ///
    /// This does not make `max_width` a line width the formatter honors. It
    /// bounds only the text this accumulator itself builds; once the decision
    /// comes back "does not fit", the multi-line renderers take over, and
    /// they enforce no width budget at all (deliberately — a later phase).
    /// So a line can still exceed `max_width`, and does: over 400 realistic
    /// forms, lines past the configured width went 96/400 before charging
    /// `start_column` to 92/400 after, and the widest line got *wider*
    /// (135 to 150 columns). Do not read this as a width guarantee.
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

/// A [`TopLevelItem`] with every piece of text already rendered, but not yet
/// joined: [`Formatter::format`] needs every item's rendered width before it
/// can decide a trailing comment's column (FR-005) or the separator before
/// the next item (FR-006), so rendering happens in this intermediate form
/// first and assembly happens once every item is known.
enum RenderedItem {
    Form {
        /// One already-rendered line per leading comment.
        leading: Vec<String>,
        /// The form's own rendering — verbatim source slice or freshly
        /// formatted text. May itself span multiple lines.
        body: String,
        /// Display width (via [`UnicodeWidthStr::width`]) of `body`'s last
        /// line, i.e. the column a same-line trailing comment starts from.
        body_last_line_width: usize,
        /// The trailing comment's own rendered text, without the leading
        /// padding that separates it from `body`.
        trailing: Option<String>,
    },
    Comments(Vec<String>),
}

impl RenderedItem {
    /// Whether this item is a form with a same-line trailing comment — the
    /// only shape [`Formatter::trailing_comment_columns`] ever aligns.
    const fn has_trailing_comment(&self) -> bool {
        matches!(
            self,
            Self::Form {
                trailing: Some(_),
                ..
            }
        )
    }

    /// The column `body` occupies up to, for width comparisons across a run
    /// of items being aligned together. `0` for a standalone comment run,
    /// which never enters an alignment group.
    const fn body_last_line_width(&self) -> usize {
        match self {
            Self::Form {
                body_last_line_width,
                ..
            } => *body_last_line_width,
            Self::Comments(_) => 0,
        }
    }

    /// Appends this item's rendering to `output`, padding a trailing comment
    /// out to `column` when one was assigned.
    ///
    /// `column` is `None` whenever comment-column alignment is disabled, or
    /// this item's trailing comment (if any) sits outside every alignment
    /// group — in both cases this falls back to the one-space separator that
    /// is the formatter's behavior with the feature off entirely.
    fn render(&self, column: Option<usize>, output: &mut String) {
        match self {
            Self::Form {
                leading,
                body,
                body_last_line_width,
                trailing,
            } => {
                for line in leading {
                    output.push_str(line);
                    output.push('\n');
                }
                // `body` was rendered into its own empty buffer, so every
                // column inside it was measured from 0 — see
                // `Formatter::render_item`. Splicing it at a nonzero column
                // would leave every line after its first indented relative to
                // the wrong origin, silently and without any error. Every
                // path into here does arrive at column 0 (an empty `output`,
                // a `blank_line_separator` that always ends in a newline, or
                // the leading-comment loop just above, which does too); this
                // fails loudly if a future change stops doing so.
                debug_assert_eq!(
                    Formatter::last_line_width(output),
                    0,
                    "top-level form body spliced at a nonzero column, but it was rendered against column 0"
                );
                output.push_str(body);
                if let Some(comment) = trailing {
                    // At least one space always separates the form from its
                    // comment, even when the form's own last line already
                    // reaches or passes the target column — the alignment
                    // column is an aspiration, not something that may ever
                    // delete a space to reach.
                    let padding = column
                        .map_or(1, |target| target.saturating_sub(*body_last_line_width))
                        .max(1);
                    output.push_str(&" ".repeat(padding));
                    output.push_str(comment);
                }
            }
            Self::Comments(lines) => {
                for (position, line) in lines.iter().enumerate() {
                    if position > 0 {
                        output.push('\n');
                    }
                    output.push_str(line);
                }
            }
        }
    }
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
            comment_column: None,
            max_blank_lines: None,
            indent_overrides: Vec::new(),
            width_profiles: Vec::new(),
            quote_style: ReaderPrefixStyle::Shorthand,
            numeric_literal_case: NumericLiteralCase::Preserve,
            align_clause_values: false,
            insert_final_newline: true,
            trim_trailing_whitespace: true,
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

    /// Aligns same-line trailing comments to `column` instead of the
    /// compiled-in single space.
    ///
    /// `0` means auto: each contiguous run of adjacent, trailing-commented
    /// top-level forms (see [`Self::trailing_comment_columns`] for exactly
    /// what breaks a run) aligns to one column past its own widest form,
    /// independently of every other run. Any other value is a fixed column
    /// every trailing comment in the document aligns to, run or no run — a
    /// fixed column is an absolute position, so grouping does not apply to
    /// it. Either way, a form whose own text already reaches or passes the
    /// target column still gets exactly one space before its comment rather
    /// than a column violated.
    ///
    /// Unset (the default from [`Self::with_dialect`]) reproduces the
    /// formatter's original behavior exactly: one space, no alignment.
    #[must_use]
    pub const fn with_comment_column(mut self, column: usize) -> Self {
        self.comment_column = Some(column);
        self
    }

    /// Preserves up to `max` consecutive blank lines from the source between
    /// top-level forms, instead of always collapsing to exactly one.
    ///
    /// `0` means no blank line is ever inserted between top-level forms,
    /// regardless of the source. Above the source's actual blank-line count,
    /// this has no effect — a gap of one blank line stays one blank line
    /// whether `max` is 1 or 100; only a gap wider than `max` is collapsed
    /// down to it.
    ///
    /// Unset (the default from [`Self::with_dialect`]) reproduces the
    /// formatter's original behavior exactly: every gap, blank or not,
    /// becomes exactly one blank line.
    #[must_use]
    pub const fn with_max_blank_lines(mut self, max: usize) -> Self {
        self.max_blank_lines = Some(max);
        self
    }

    #[must_use]
    pub fn format(&self, tree: &SyntaxTree) -> String {
        let items = self.plan_top_level(tree);
        if items.is_empty() {
            return String::new();
        }
        let rendered: Vec<RenderedItem> = items
            .iter()
            .map(|item| self.render_item(tree, item))
            .collect();
        let columns = self.trailing_comment_columns(tree, &items, &rendered);

        let mut output = String::new();
        for (position, item) in rendered.iter().enumerate() {
            if position > 0 {
                output.push_str(&self.blank_line_separator(
                    tree,
                    &items[position - 1],
                    &items[position],
                ));
            }
            item.render(columns[position], &mut output);
        }
        if self.insert_final_newline {
            output.push('\n');
        }
        output
    }

    /// The separator text between two adjacent top-level items: `"\n"` for
    /// every blank line the result should carry, plus one more `"\n"` to end
    /// the previous item's own last line.
    ///
    /// Disabled ([`Self::max_blank_lines`] unset) reproduces the original
    /// hardcoded `"\n\n"` — exactly one blank line — regardless of what the
    /// source had between `prev` and `cur`. Enabled, the source's actual
    /// blank-line count between them is read back via
    /// [`Self::blank_lines_between`] and clamped to the configured maximum.
    fn blank_line_separator(
        &self,
        tree: &SyntaxTree,
        prev: &TopLevelItem,
        cur: &TopLevelItem,
    ) -> String {
        let blank_lines = match self.max_blank_lines {
            None => 1,
            Some(max) => Self::blank_lines_between(tree, prev, cur).min(max),
        };
        "\n".repeat(blank_lines + 1)
    }

    /// How many blank lines separate `prev` and `cur` in the source.
    ///
    /// The gap between one item's rendered end and the next item's rendered
    /// start is whitespace only — every non-whitespace byte in it belongs to
    /// a comment or a node, and both are already accounted for as part of one
    /// item or the other — so its newline count minus one *is* the blank-line
    /// count, with no risk of miscounting something that is not whitespace.
    fn blank_lines_between(tree: &SyntaxTree, prev: &TopLevelItem, cur: &TopLevelItem) -> usize {
        let prev_end = Self::item_end_offset(tree, prev);
        let cur_start = Self::item_start_offset(tree, cur);
        if cur_start <= prev_end {
            return 0;
        }
        tree.source[prev_end..cur_start]
            .matches('\n')
            .count()
            .saturating_sub(1)
    }

    /// Byte offset one past `item`'s last rendered character in the source:
    /// its trailing comment's end when it has one, otherwise its last node's.
    fn item_end_offset(tree: &SyntaxTree, item: &TopLevelItem) -> usize {
        match item {
            TopLevelItem::Form {
                end_node_id,
                trailing,
                ..
            } => trailing.map_or_else(
                || tree.node(*end_node_id).span.end().get(),
                |comment| tree.comments[comment].span.end().get(),
            ),
            TopLevelItem::Comments(indices) => {
                let last = *indices.last().expect("comment run is never empty");
                tree.comments[last].span.end().get()
            }
        }
    }

    /// Byte offset of `item`'s first rendered character in the source: its
    /// first leading comment's start when it has one, otherwise its node's.
    fn item_start_offset(tree: &SyntaxTree, item: &TopLevelItem) -> usize {
        match item {
            TopLevelItem::Form {
                node_id, leading, ..
            } => leading.first().map_or_else(
                || tree.node(*node_id).span.start().get(),
                |&comment| tree.comments[comment].span.start().get(),
            ),
            TopLevelItem::Comments(indices) => {
                let first = *indices.first().expect("comment run is never empty");
                tree.comments[first].span.start().get()
            }
        }
    }

    /// The target column, if any, each item's trailing comment should pad
    /// out to — `None` everywhere when [`Self::comment_column`] is unset.
    ///
    /// A fixed column (`comment_column` other than `0`) applies uniformly to
    /// every trailing comment in the document: it is an absolute position,
    /// so adjacency between forms does not matter to it.
    ///
    /// Auto (`comment_column` is `0`) instead groups items into runs and
    /// aligns each run to one column past its own widest form. A run is a
    /// maximal sequence of consecutive top-level [`TopLevelItem::Form`]
    /// items that each carry a trailing comment, with no blank line rendered
    /// between any two adjacent members — the same "contiguous commented
    /// lines" rule a line-oriented formatter would apply, adapted to this
    /// formatter's top-level-forms-only data model. A form without a
    /// trailing comment, or a rendered blank line, ends the run there; the
    /// next run (if any) starts its own alignment independently.
    ///
    /// "Rendered" is deliberate, not the source's raw blank-line count:
    /// adjacency is decided by calling [`Self::blank_line_separator`] itself
    /// and checking whether *it* produced a blank line, the same call
    /// [`Self::format`] makes to build the separator that actually lands in
    /// the output. Two forms this pass renders back-to-back with no blank
    /// line between them are a run; anything else is not. Basing it on the
    /// source's raw gap instead would break idempotency: with
    /// [`Self::max_blank_lines`] unset, every gap renders as exactly one
    /// blank line regardless of what the source had (`Self::format`'s
    /// original, preserved behavior), so a source gap of zero never survives
    /// a reformat — a run computed from the raw gap would group on the first
    /// pass and never again. Reusing `blank_line_separator` instead ties the
    /// two decisions to the same fixed point `blank_line_separator` already
    /// is (`min(actual, max)` is idempotent, and the unset case is a
    /// constant), so grouping agrees with itself on every pass. The
    /// trade-off is explicit: with `max_blank_lines` left unset, forms are
    /// never rendered back-to-back at all, so auto alignment groups nothing
    /// until the caller also sets `max_blank_lines` to let some gaps render
    /// as zero blank lines — the one point where FR-005 and FR-006 are not
    /// independent of each other.
    fn trailing_comment_columns(
        &self,
        tree: &SyntaxTree,
        items: &[TopLevelItem],
        rendered: &[RenderedItem],
    ) -> Vec<Option<usize>> {
        let mut columns = vec![None; items.len()];
        let Some(comment_column) = self.comment_column else {
            return columns;
        };

        if comment_column != 0 {
            for (index, item) in rendered.iter().enumerate() {
                if item.has_trailing_comment() {
                    columns[index] = Some(comment_column);
                }
            }
            return columns;
        }

        let mut index = 0usize;
        while index < items.len() {
            if !rendered[index].has_trailing_comment() {
                index += 1;
                continue;
            }
            let run_start = index;
            let mut run_end = index;
            while run_end + 1 < items.len()
                && rendered[run_end + 1].has_trailing_comment()
                && self.blank_line_separator(tree, &items[run_end], &items[run_end + 1]) == "\n"
            {
                run_end += 1;
            }

            let widest = (run_start..=run_end)
                .map(|member| rendered[member].body_last_line_width())
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
                .reader_prefixes()
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
                let comment_line_start = tree.source[..comment_start]
                    .rfind('\n')
                    .map_or(0, |newline| newline + 1);
                let same_line = comment_start >= end && comment_line_start < end;
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

    /// Renders one [`TopLevelItem`] into a [`RenderedItem`], without yet
    /// deciding the padding before a trailing comment or the separator
    /// before the next item — both need every item's rendering measured
    /// first, which [`Self::format`] does across the whole document before
    /// assembling anything.
    ///
    /// # Precondition
    ///
    /// The rendered body must end up spliced into the document **at column
    /// 0**. Rendering here goes into a fresh, empty `String`, so every
    /// [`Self::last_line_width`] call underneath it measures that temporary
    /// buffer rather than the real document — the column model is correct
    /// only because the two agree, which they do only at column 0. Nothing in
    /// the type system holds this; [`RenderedItem::render`] carries the
    /// matching `debug_assert!`. A future caller that wants to splice a body
    /// partway across a line has to thread a start column through
    /// [`Self::format_node`] instead of relying on this.
    fn render_item(&self, tree: &SyntaxTree, item: &TopLevelItem) -> RenderedItem {
        let comments = &tree.comments;
        match item {
            TopLevelItem::Form {
                node_id,
                end_node_id,
                leading,
                trailing,
                verbatim,
            } => {
                let leading = leading
                    .iter()
                    .map(|&comment| self.render_comment_text(&comments[comment].text, 0))
                    .collect();

                let mut body = String::new();
                if *verbatim {
                    let start = tree.node(*node_id).span.start().get();
                    let end = tree.node(*end_node_id).span.end().get();
                    body.push_str(&tree.source[start..end]);
                } else {
                    self.format_node(tree, *node_id, 0, &mut body);
                }
                let body_last_line_width = Self::last_line_width(&body);

                let trailing =
                    trailing.map(|comment| self.render_comment_text(&comments[comment].text, 0));

                RenderedItem::Form {
                    leading,
                    body,
                    body_last_line_width,
                    trailing,
                }
            }
            TopLevelItem::Comments(indices) => RenderedItem::Comments(
                indices
                    .iter()
                    .map(|&comment| self.render_comment_text(&comments[comment].text, 0))
                    .collect(),
            ),
        }
    }

    /// Display width (via [`UnicodeWidthStr::width`]) of `text`'s last line.
    ///
    /// Called on the output buffer, this is the column the next character
    /// appended to it will land on — the formatter's whole column model. A
    /// form's opening delimiter goes wherever the line has already reached,
    /// which `depth * indent` only agrees with when nothing preceded the form
    /// on its line; a form hugged onto its operator's line, or one behind a
    /// reader prefix, starts further right and every line it emits has to be
    /// measured from there. Called on a rendered form, it is also the column
    /// at which a same-line trailing comment would start.
    pub(super) fn last_line_width(text: &str) -> usize {
        let last_line = text.rsplit('\n').next().unwrap_or("");
        UnicodeWidthStr::width(last_line)
    }

    /// Ends the current line and indents the next one out to `column`.
    ///
    /// The indent is copied in `SPACES`-sized slices rather than built a
    /// character at a time; see that constant for why, and for the numbers.
    pub(super) fn break_to_column(column: usize, output: &mut String) {
        output.push('\n');
        let mut remaining = column;
        while remaining > SPACES.len() {
            output.push_str(SPACES);
            remaining -= SPACES.len();
        }
        output.push_str(&SPACES[..remaining]);
    }

    /// Trims a comment's trailing whitespace and, when enabled, realigns a
    /// `#|...|#` block comment's interior lines to `depth`.
    ///
    /// Every call site in [`Self::render_item`] is a standalone or
    /// trailing comment between top-level forms, so `depth` is always `0`
    /// today; the depth parameter still exists because the transformation
    /// itself has nothing top-level-specific about it.
    ///
    /// Trailing whitespace is trimmed unless [`Self::trim_trailing_whitespace`]
    /// is off — this is the one span outside a string literal's own text
    /// where the formatter ever had trailing whitespace of its own to trim,
    /// since every other span is either laid out fresh (never carries stray
    /// trailing whitespace to begin with) or copied verbatim from source for
    /// an unrelated reason (an opaque reader form, or a top-level item kept
    /// exactly as written because it carries an interior comment) and stays
    /// untouched either way.
    fn render_comment_text(&self, text: &str, depth: usize) -> String {
        let trimmed = if self.trim_trailing_whitespace {
            text.trim_end()
        } else {
            text
        };
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
            // An atom's own text — after any embedded reader-prefix chars
            // `node.symbol_offset` skips — is always copied from source
            // (through `numeric_literal_text`'s no-op-unless-numeric
            // recasing, its only transformation) rather than re-derived.
            // This is a deliberate contract, not an oversight: Lisp readers
            // give string-literal whitespace exact semantics (CLHS 2.4.6 —
            // the characters between the quotes are the string's value,
            // verbatim), so a multiline string's interior indentation must
            // never be reindented the way surrounding code is. Because a
            // string atom's `span` covers its opening and closing `"` and
            // everything between them, and every other atom kind (a symbol,
            // a number, a character literal) is just as safe to copy
            // verbatim, this single code path handles the string-literal
            // contract for free rather than needing its own case.
            NodeKind::Atom => {
                if let Some(heads) = self.canonical_prefix_heads(node) {
                    self.write_canonical_opens(&heads, output);
                    let text = node.span.slice(&tree.source);
                    let content = self.numeric_literal_text(&text[node.symbol_offset as usize..]);
                    output.push_str(&content);
                    Self::write_canonical_closes(&heads, output);
                } else if self.numeric_literal_case == NumericLiteralCase::Preserve {
                    // No reader prefix to canonicalize and no case recasing to
                    // apply: copy the atom's whole span verbatim in one push,
                    // same as before numeric-literal-case existed. Splitting
                    // this into a symbol_offset-relative slice plus a
                    // numeric_literal_text call on every atom regardless of
                    // whether the feature is in use was a real, benchmarked
                    // regression (~13% on `clean/forms/*`) — Preserve is the
                    // default, so this fast path is the common case.
                    output.push_str(node.span.slice(&tree.source));
                } else {
                    let text = node.span.slice(&tree.source);
                    output.push_str(&text[..node.symbol_offset as usize]);
                    output
                        .push_str(&self.numeric_literal_text(&text[node.symbol_offset as usize..]));
                }
            }
            NodeKind::List if node.children.is_empty() => {
                if self.is_opaque_reader_form(node) {
                    output.push_str(node.span.slice(&tree.source));
                    return;
                }
                let heads = self.canonical_prefix_heads(node);
                if let Some(heads) = &heads {
                    self.write_canonical_opens(heads, output);
                } else {
                    self.write_reader_prefixes(tree, node, output);
                }
                let delimiter = self.list_delimiter(node);
                output.push(delimiter.open());
                output.push(delimiter.close());
                if let Some(heads) = &heads {
                    Self::write_canonical_closes(heads, output);
                }
            }
            NodeKind::List => {
                // Measured before any reader prefix is written, because
                // `compact_node` writes the prefix spans itself and so must
                // charge them against the same budget exactly once.
                if let Some(inline) = self.inline_list(tree, node_id, Self::last_line_width(output))
                {
                    output.push_str(&inline);
                    return;
                }
                if self.is_opaque_reader_form(node) {
                    output.push_str(node.span.slice(&tree.source));
                    return;
                }
                let heads = self.canonical_prefix_heads(node);
                if let Some(heads) = &heads {
                    self.write_canonical_opens(heads, output);
                } else {
                    self.write_reader_prefixes(tree, node, output);
                }
                // A reader conditional is keyed off its prefix, not its head:
                // the head of `#?(:clj a :cljs b)` is the feature keyword.
                // (Never true together with `heads`: `#?`/`#?@` never
                // canonicalize, so `heads` is `None` whenever this fires.)
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
                            self.format_local_callable_form(tree, node_id, depth, output);
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
                            self.format_do_form(tree, node_id, depth, output);
                        }
                        ListStyle::Prog => {
                            self.format_prog_form(tree, node_id, depth, output);
                        }
                        ListStyle::Declaration => {
                            self.format_declaration_form(tree, node_id, depth, output);
                        }
                        ListStyle::PairAssignment => {
                            self.format_pair_assignment_form(tree, node_id, depth, output);
                        }
                        ListStyle::Loop => {
                            self.format_loop_form(tree, node_id, depth, output);
                        }
                        ListStyle::HeadBody => {
                            self.format_head_body(tree, node_id, depth, output);
                        }
                        ListStyle::If => {
                            self.format_prefix_body(tree, node_id, depth, 2, output);
                        }
                        ListStyle::IfAligned => {
                            self.format_if_aligned(tree, node_id, depth, output);
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
                            self.format_clojure_threading(tree, node_id, depth, prefix_len, output);
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
                if let Some(heads) = &heads {
                    Self::write_canonical_closes(heads, output);
                }
            }
        }
    }

    /// Writes `(head1 (head2 ... (headN ` for a canonical prefix expansion,
    /// outermost first — the opening half of [`Self::write_canonical_closes`].
    fn write_canonical_opens(&self, heads: &[&'static str], output: &mut String) {
        for head in heads {
            output.push('(');
            output.push_str(head);
            output.push(' ');
        }
    }

    /// Writes the `)`s closing a canonical prefix expansion
    /// [`Self::write_canonical_opens`] began.
    fn write_canonical_closes(heads: &[&'static str], output: &mut String) {
        for _ in heads {
            output.push(')');
        }
    }

    /// The canonical list-form heads `node`'s reader-prefix stack expands to,
    /// outermost first, when [`ReaderPrefixStyle::Canonical`] is set and
    /// every prefix in the stack has a portable list spelling for
    /// `self.dialect` (see `quote_edit`'s module doc for why quasiquote and
    /// unquote never do). `None` otherwise — including when the node carries
    /// no reader prefix at all — in which case the prefix stack keeps its
    /// shorthand spelling. A stack is canonicalized all together or not at
    /// all: one mixing a canonicalizable prefix with one that is not (e.g.
    /// `` '`x ``) is left exactly as written rather than partly rewritten.
    pub(super) fn canonical_prefix_heads(&self, node: &Node) -> Option<Vec<&'static str>> {
        if self.quote_style != ReaderPrefixStyle::Canonical || node.reader_prefixes().is_empty() {
            return None;
        }
        node.reader_prefixes()
            .iter()
            .map(|prefix| canonical_quote_head(self.dialect, *prefix))
            .collect()
    }

    pub(super) fn inline_list(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        start_column: usize,
    ) -> Option<String> {
        let head = self.head_text(tree, node_id);
        if head.is_some_and(|head| self.style_for_head(head) != ListStyle::General) {
            return None;
        }
        self.compact_node(tree, node_id, start_column)
    }

    /// Flattens `node_id` onto one line, or `None` when it does not fit.
    ///
    /// `start_column` is the display column the returned text would be
    /// written at. It is charged against the budget up front because this
    /// builds into a separate accumulator that starts empty and so cannot
    /// observe the cursor itself: without it a form hugged onto an operator's
    /// line gets a fresh full-width allowance for the fit decision, and this
    /// returns `Some` for text that cannot actually fit where it will land.
    /// Callers that only want a subtree's text measured, rather than a
    /// decision about a line, pass `0`.
    ///
    /// What `start_column` fixes is that decision, and only it. Returning
    /// `None` hands the node to a multi-line renderer, and those enforce no
    /// width budget whatsoever, so the emitted line may still pass
    /// `max_width` — see [`Bounded::with_initial_width`] for the measured
    /// residual overrun. This is a *decision* input, not a width bound.
    pub(super) fn compact_node(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        start_column: usize,
    ) -> Option<String> {
        enum Action {
            Node(NodeId),
            Separator,
            Close(char),
        }

        // Resolved once, from `node_id`'s own head — not re-resolved per
        // descendant, since the whole subtree this call flattens shares one
        // running [`Bounded`] budget. See `Formatter::effective_max_width`.
        let max_width = self.effective_max_width(tree, node_id);
        let mut output = Bounded::with_initial_width(start_column);
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
                            if let Some(heads) = self.canonical_prefix_heads(node) {
                                let text = node.span.slice(&tree.source);
                                let content =
                                    self.numeric_literal_text(&text[node.symbol_offset as usize..]);
                                for head in &heads {
                                    output.push_char('(', max_width)?;
                                    output.push_str(head, max_width)?;
                                    output.push_char(' ', max_width)?;
                                }
                                output.push_str(&content, max_width)?;
                                for _ in &heads {
                                    output.push_char(')', max_width)?;
                                }
                            } else if self.numeric_literal_case == NumericLiteralCase::Preserve {
                                // See the matching fast path in `format_node`:
                                // skip the symbol_offset split and
                                // numeric_literal_text call entirely when
                                // there's nothing for either to do.
                                output.push_str(node.span.slice(&tree.source), max_width)?;
                            } else {
                                let text = node.span.slice(&tree.source);
                                output.push_str(&text[..node.symbol_offset as usize], max_width)?;
                                output.push_str(
                                    &self
                                        .numeric_literal_text(&text[node.symbol_offset as usize..]),
                                    max_width,
                                )?;
                            }
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

                            let heads = self.canonical_prefix_heads(node);
                            if let Some(heads) = &heads {
                                // Pushed before `Close(delimiter.close())` below,
                                // so — stack being LIFO — they pop, and print,
                                // after it: `(quote (children))`, not
                                // `(quote )(children)`.
                                for _ in heads {
                                    actions.push(Action::Close(')'));
                                }
                                for head in heads {
                                    output.push_char('(', max_width)?;
                                    output.push_str(head, max_width)?;
                                    output.push_char(' ', max_width)?;
                                }
                            } else {
                                for span in node.reader_prefix_spans() {
                                    output.push_str(span.slice(&tree.source), max_width)?;
                                }
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

    /// The width budget for deciding whether `node_id` fits on one line: the
    /// `format.width-profiles` entry for its own head's resolved style, if
    /// one was configured, else `self.max_width`.
    ///
    /// In practice only ever observed for [`ListStyle::General`] (or a style
    /// [`Formatter::with_indent_overrides`] retargeted onto it): every other
    /// style already refuses to compact at all, unconditionally, before this
    /// value is ever read — this same function's `NodeKind::List` match arm,
    /// two blocks down, bails via `return None` on a non-`General` head
    /// before checking any width, for the entry node exactly as it does for
    /// every recursively visited one. `formatter::tests::formatter`'s
    /// `with_max_width_narrows_the_inline_fit_threshold` documents that same
    /// restriction for the compiled-in `max_width`. This function still
    /// resolves every style generically, rather than special-casing
    /// `General`, so a future phase that teaches another style to compact
    /// gets a working width profile for free.
    fn effective_max_width(&self, tree: &SyntaxTree, node_id: NodeId) -> usize {
        let Some(head) = self.head_text(tree, node_id) else {
            return self.max_width;
        };
        let style = self.style_for_head(head);
        // Reverse search, matching `Formatter::style_for_head`: per this
        // struct's `width_profiles` field doc, a later entry for the same
        // style wins over an earlier one.
        self.width_profiles
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate == style)
            .map_or(self.max_width, |(_, width)| *width)
    }

    pub(super) fn format_inline_or_node(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        output: &mut String,
    ) {
        if let Some(inline) = self.compact_node(tree, node_id, Self::last_line_width(output)) {
            output.push_str(&inline);
        } else {
            self.format_node(tree, node_id, depth, output);
        }
    }

    pub(super) fn head_text<'a>(&self, tree: &'a SyntaxTree, node_id: NodeId) -> Option<&'a str> {
        let node = tree.node(node_id);
        let first = *node.children.first()?;
        let first = tree.node(first);
        (first.kind == NodeKind::Atom && first.reader_prefixes().is_empty())
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
        for span in node.reader_prefix_spans() {
            output.push_str(span.slice(&tree.source));
        }
    }

    /// Whether `node` carries reader-prefix text that only
    /// [`Self::format_node`] emits.
    ///
    /// [`Self::format_node`] writes a node's prefixes (or their canonical
    /// expansion) *before* it dispatches on the node's head, so every
    /// shape-specific renderer in `formatter/lists` may open its own subject's
    /// delimiter freely: the prefix is already out. What those renderers may
    /// not do is open a *child's* delimiter, because nothing wrote that
    /// child's prefix — and six of them did exactly that, for the child they
    /// treat as a binding list, a binding entry, or a clause.
    ///
    /// The result was silent corruption rather than a visible defect:
    /// `` (let `(a b) x) `` re-emitted as `(let (a b) x)` still parses, so it
    /// passes every reparse-based write guard while meaning something else.
    ///
    /// Handing such a child back to [`Self::format_inline_or_node`] is also
    /// the right *layout* answer and not merely the safe one — `` `(a b) `` in
    /// a `let`'s binding slot is a quasiquoted datum, not a binding list, and
    /// laying it out as the shape it is not was never correct.
    ///
    /// An opaque reader form (`#+sbcl (…)`) needs no entry here: the parser
    /// gives it [`NodeKind::Atom`], so every one of those renderers already
    /// falls through its `kind != List` guard.
    pub(super) fn carries_reader_prefix(node: &Node) -> bool {
        !node.reader_prefixes().is_empty()
    }

    pub(super) fn is_opaque_reader_form(&self, node: &Node) -> bool {
        node.opaque_reader_form
            || node
                .reader_prefixes()
                .iter()
                .any(|prefix| prefix.is_opaque_reader_form())
    }

    /// `text` (an atom's content, past any embedded reader-prefix chars) with
    /// its numeric-literal marker recased per [`Self::numeric_literal_case`],
    /// or borrowed unchanged when `text` is not a numeric literal this
    /// formatter's dialect recognises. See `formatter::numeric_literal` for
    /// the grammar and the dialect gating.
    fn numeric_literal_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        super::numeric_literal::numeric_literal_text(text, self.dialect, self.numeric_literal_case)
    }

    /// `depth * indent` spaces — the formatter's last remaining depth-based
    /// indentation, kept for one caller only:
    /// [`Self::reindent_block_comment`] realigning a `#|...|#` comment's
    /// interior lines.
    ///
    /// Code layout does **not** go through here. It is column-based: a form
    /// indents from where its opening delimiter actually landed, via
    /// [`Self::last_line_width`] and [`Self::add_indent`]. `depth` survives in
    /// the layout code purely as the `MAX_RECURSIVE_FORMAT_DEPTH` recursion
    /// guard, and as the argument to this. New layout code that reaches for
    /// `depth * indent` is reintroducing the model the column model replaced.
    pub(super) fn indent(&self, depth: usize) -> String {
        " ".repeat(self.indentation_width(depth))
    }

    /// One indentation step in from `column` — a form's body column, given
    /// the column its opening delimiter landed on.
    pub(super) const fn add_indent(&self, column: usize) -> usize {
        column.saturating_add(self.indent)
    }

    /// Width of [`Self::indent`], and subject to the same restriction: this
    /// is block-comment realignment's measure, not code layout's.
    const fn indentation_width(&self, depth: usize) -> usize {
        depth.saturating_mul(self.indent)
    }
}
