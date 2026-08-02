use std::fmt;

use crate::common_lisp::common_lisp_symbol_reference_eq;
use crate::dialect::Dialect;

use super::error::{SelectionError, SexprError, SexprResult, StructureError};
use super::parser::{ParseError, Parser};
use super::types::{ByteOffset, ByteSpan, Delimiter, ExpressionPath, NodeId, SymbolName};

/// A parsed S-expression document with tree navigation and query helpers.
///
/// # Examples
///
/// ```
/// use paredit_core_syntax::sexpr::{ExpressionPath, SyntaxTree};
///
/// let input = "(let ((value 1)) (+ value 2))";
/// let tree = SyntaxTree::parse(input).unwrap();
/// let selection = tree
///     .select_path(&ExpressionPath::from_indexes(vec![0, 2, 1]))
///     .unwrap();
///
/// assert_eq!(selection.text(), "value");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTree {
    pub(in crate::sexpr) nodes: Vec<Node>,
    /// Comments discovered during parsing, in source order. They are kept
    /// outside the node tree so structural refactors that walk `children` never
    /// have to reason about them; only the canonical formatter re-emits them.
    pub(in crate::sexpr) comments: Vec<Comment>,
    /// The exact source text the tree was parsed from, used by the formatter to
    /// slice comment-bearing forms verbatim and to measure line breaks.
    pub(in crate::sexpr) source: String,
}

/// A comment captured verbatim during parsing together with the placement
/// metadata the formatter needs to re-emit it without losing information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sexpr) struct Comment {
    /// Byte range of the comment in the original source.
    pub(in crate::sexpr) span: ByteSpan,
    /// Exact comment text (`; ...`, `#| ... |#`, `#; <form>`, or `#_<form>`),
    /// trailing whitespace preserved as parsed.
    pub(in crate::sexpr) text: String,
    /// `true` when only whitespace precedes the comment on its source line, i.e.
    /// it stands on its own line rather than trailing code.
    pub(in crate::sexpr) own_line: bool,
}

/// A borrowed view of one comment the parse recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceComment<'a> {
    span: ByteSpan,
    text: &'a str,
    own_line: bool,
}

impl<'a> SourceComment<'a> {
    /// The comment's byte range in the source it was parsed from.
    #[must_use]
    pub const fn span(self) -> ByteSpan {
        self.span
    }

    /// The comment text exactly as written, delimiters included.
    #[must_use]
    pub const fn text(self) -> &'a str {
        self.text
    }

    /// Whether only whitespace precedes the comment on its line.
    ///
    /// An Emacs Lisp autoload cookie only counts when this is true: the
    /// `;;;###autoload` that `loaddefs` looks for has to begin its line.
    #[must_use]
    pub const fn own_line(self) -> bool {
        self.own_line
    }
}

/// The reader sugar on one node, and the exact source ranges it occupied.
///
/// Two parallel vectors, always the same length: the normalized semantics and
/// the spellings they were written with, kept apart so dialect-specific
/// spellings round-trip.
///
/// Boxed as a unit inside [`Node`] rather than stored inline, because the
/// overwhelming majority of nodes have no reader sugar at all and two empty
/// `Vec`s still cost 48 bytes of headers each time. Behind an `Option<Box<_>>`
/// the common case pays eight, and the rare prefixed node pays one extra
/// allocation — which is the right trade at roughly one node per six source
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sexpr) struct ReaderPrefixes {
    pub(in crate::sexpr) kinds: Vec<ReaderPrefix>,
    pub(in crate::sexpr) spans: Vec<ByteSpan>,
}

impl ReaderPrefixes {
    /// The value a [`Node`]'s `reader` field takes for the given sugar.
    ///
    /// Returns `None` for the empty case rather than an allocated pair of
    /// empty vectors. Funnelling construction through here is what keeps the
    /// `Some(empty)` state — which reads identically through the accessors on
    /// [`Node`] but costs the allocation this indirection exists to avoid —
    /// from ever being built.
    pub(in crate::sexpr) fn boxed(
        kinds: Vec<ReaderPrefix>,
        spans: Vec<ByteSpan>,
    ) -> Option<Box<Self>> {
        debug_assert_eq!(
            kinds.len(),
            spans.len(),
            "reader prefix kinds and spans are parallel"
        );
        if kinds.is_empty() {
            return None;
        }
        Some(Box::new(Self { kinds, spans }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sexpr) struct Node {
    pub(in crate::sexpr) kind: NodeKind,
    pub(in crate::sexpr) delimiter: Option<Delimiter>,
    /// `None` for a node with no reader sugar, which is nearly all of them.
    /// Read through [`Node::reader_prefixes`] and
    /// [`Node::reader_prefix_spans`], which flatten the absent case to an
    /// empty slice.
    pub(in crate::sexpr) reader: Option<Box<ReaderPrefixes>>,
    pub(in crate::sexpr) parent: Option<NodeId>,
    pub(in crate::sexpr) children: Vec<NodeId>,
    pub(in crate::sexpr) span: ByteSpan,
    pub(in crate::sexpr) open: Option<ByteOffset>,
    pub(in crate::sexpr) close: Option<ByteOffset>,
    /// Byte offset from `span.start()` to where an atom's own symbol content
    /// begins, i.e. past its reader prefixes *and* any trivia (whitespace or
    /// comments) between the last prefix and the symbol. Reader prefixes are
    /// followed by `skip_trivia()` during parsing (`#' foo` is valid, if
    /// unusual, syntax), so this cannot be recovered later by summing each
    /// prefix's fixed source length — it must be recorded while parsing.
    /// Meaningless (`0`) for non-atom nodes.
    ///
    /// A `u32` for the same reason [`ByteOffset`] is: it is an offset into the
    /// same bounded document, and as the only remaining eight-byte scalar it
    /// was costing a further four bytes of tail padding.
    pub(in crate::sexpr) symbol_offset: u32,
    /// Reader forms that consume multiple datums are represented by one
    /// verbatim atom node so their payload cannot become editable siblings.
    pub(in crate::sexpr) opaque_reader_form: bool,
}

impl Node {
    /// The node's reader sugar, or an empty slice when it has none.
    pub(in crate::sexpr) fn reader_prefixes(&self) -> &[ReaderPrefix] {
        self.reader.as_ref().map_or(&[], |reader| &reader.kinds)
    }

    /// The source ranges of [`Self::reader_prefixes`], one per entry.
    pub(in crate::sexpr) fn reader_prefix_spans(&self) -> &[ByteSpan] {
        self.reader.as_ref().map_or(&[], |reader| &reader.spans)
    }
}

/// The node arena dominates a parse's memory, so `Node`'s layout is a
/// documented property rather than an accident.
///
/// It was 152 bytes before this bound existed. Nearly all of the difference is
/// three changes worth keeping: `ByteOffset` and `NodeId` narrowed to `u32`
/// (see their documentation for why four gigabytes is a fact here, not a
/// hope), and the two reader-prefix vectors moved behind one `Option<Box<_>>`.
///
/// At roughly one node per six source bytes, every eight bytes added here
/// costs about 1.3x the document's own size in resident memory — so a field
/// added without thought is not a small regression. If a new field genuinely
/// belongs on every node, raise this number deliberately and say why;
/// `tests/parse_memory.rs` measures what it costs.
const _: () = assert!(
    std::mem::size_of::<Node>() <= 72,
    "Node grew past its documented 72-byte budget"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::sexpr) enum NodeKind {
    Root,
    List,
    Atom,
}

/// Reader sugar that prefixes an expression in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderPrefix {
    Quote,
    Quasiquote,
    Unquote,
    UnquoteSplicing,
    Function,
    ReadEval,
    /// A bare `#` immediately before an open delimiter: Common Lisp/Scheme
    /// vector literals (`#(1 2 3)`) and Clojure set (`#{1 2}`) or anonymous
    /// function (`#(+ % 1)`) literals. All three dialects glue `#` directly
    /// onto the following collection with no space, so this keeps the `#`
    /// attached to its list instead of scanning as a disconnected atom.
    HashLiteral,
    /// Clojure metadata sugar (`^{:doc "x"}`, `^:private`, `^String`)
    /// prefixing the map, keyword, or symbol that carries the metadata.
    Metadata,
    /// Clojure reader conditional (`#?(:clj a :cljs b)`).
    ReaderConditional,
    /// Clojure splicing reader conditional (`#?@(:clj [a] :cljs [b])`).
    ReaderConditionalSplicing,
}

impl ReaderPrefix {
    /// Returns the exact source spelling for this reader prefix.
    #[must_use]
    pub const fn as_source(self) -> &'static str {
        match self {
            Self::Quote => "'",
            Self::Quasiquote => "`",
            Self::Unquote => ",",
            Self::UnquoteSplicing => ",@",
            Self::Function => "#'",
            Self::ReadEval => "#.",
            Self::HashLiteral => "#",
            Self::Metadata => "^",
            Self::ReaderConditional => "#?",
            Self::ReaderConditionalSplicing => "#?@",
        }
    }

    /// Returns true when this prefix makes the following form opaque to structural refactors.
    #[must_use]
    pub const fn is_opaque_reader_form(self) -> bool {
        matches!(self, Self::ReadEval)
    }
}

/// Summary of one root-level list in outline-oriented reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub path: ExpressionPath,
    pub span: ByteSpan,
    pub head: Option<String>,
    pub definition_like: bool,
}

/// One atom plus its tree path and byte span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomOccurrence {
    pub path: ExpressionPath,
    pub span: ByteSpan,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub struct BorrowedAtomOccurrence<'a> {
    node_id: NodeId,
    pub span: ByteSpan,
    pub text: &'a str,
}

// Public since the extraction: it was `pub(in crate::domain)`, a visibility
// that cannot cross a crate boundary, so `missing_debug_implementations`
// applies to it for the first time.
#[derive(Debug)]
pub struct AtomOccurrenceIndex<'a> {
    parent_steps: Vec<Option<(NodeId, usize)>>,
    occurrences: Vec<BorrowedAtomOccurrence<'a>>,
    quoted_designators: Vec<BorrowedAtomOccurrence<'a>>,
}

impl AtomOccurrenceIndex<'_> {
    #[must_use]
    pub fn occurrences(&self) -> &[BorrowedAtomOccurrence<'_>] {
        &self.occurrences
    }

    fn rename_occurrences(&self) -> impl Iterator<Item = &BorrowedAtomOccurrence<'_>> {
        self.occurrences.iter().chain(&self.quoted_designators)
    }

    #[must_use]
    pub fn path_for_span(&self, span: ByteSpan) -> Option<ExpressionPath> {
        let occurrence = self.find_by_span(span)?;
        Some(self.path_for_node(occurrence.node_id))
    }

    #[must_use]
    pub fn last_index_for_span(&self, span: ByteSpan) -> Option<usize> {
        let occurrence = self.find_by_span(span)?;
        self.parent_steps[occurrence.node_id.get()].map(|(_, index)| index)
    }

    fn find_by_span(&self, span: ByteSpan) -> Option<&BorrowedAtomOccurrence<'_>> {
        let key = (span.start(), span.end());
        self.occurrences
            .binary_search_by_key(&key, |occurrence| {
                (occurrence.span.start(), occurrence.span.end())
            })
            .ok()
            .map(|index| &self.occurrences[index])
    }

    fn path_for_node(&self, node_id: NodeId) -> ExpressionPath {
        let mut indexes = Vec::new();
        let mut cursor = Some(node_id);
        while let Some(current) = cursor {
            let Some((parent, index)) = self.parent_steps[current.get()] else {
                break;
            };
            indexes.push(index);
            cursor = (parent != NodeId::ROOT).then_some(parent);
        }
        indexes.reverse();
        ExpressionPath::from_indexes(indexes)
    }
}

/// The high-level shape of an expression node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpressionKind {
    Root,
    List,
    Atom,
}

/// Immutable tree view data for one expression and its descendants.
pub struct ExpressionView {
    pub kind: ExpressionKind,
    pub delimiter: Option<Delimiter>,
    pub reader_prefixes: Vec<ReaderPrefix>,
    pub span: ByteSpan,
    /// The expression span after its reader prefixes and intervening trivia.
    ///
    /// For lists this starts at the opening delimiter; for atoms it starts at
    /// the symbol content. Structural transformations can replace this span
    /// without detaching reader prefixes from their expression.
    pub content_span: ByteSpan,
    pub text: Option<String>,
    pub children: Vec<ExpressionView>,
    /// Byte offset from `span.start()` to where an atom's own symbol content
    /// begins, past its reader prefixes and any intervening trivia. `0` for
    /// non-atom nodes.
    pub symbol_offset: usize,
}

impl Clone for ExpressionView {
    fn clone(&self) -> Self {
        let mut frames = vec![(self, false)];
        let mut clones = Vec::new();

        while let Some((view, expanded)) = frames.pop() {
            if !expanded {
                frames.push((view, true));
                frames.extend(view.children.iter().rev().map(|child| (child, false)));
                continue;
            }

            let children_start = clones
                .len()
                .checked_sub(view.children.len())
                .expect("expanded expression clone has all child views");
            let children = clones.split_off(children_start);
            clones.push(Self {
                kind: view.kind,
                delimiter: view.delimiter,
                reader_prefixes: view.reader_prefixes.clone(),
                span: view.span,
                content_span: view.content_span,
                text: view.text.clone(),
                children,
                symbol_offset: view.symbol_offset,
            });
        }

        clones.pop().expect("expression view clone is constructed")
    }
}

impl PartialEq for ExpressionView {
    fn eq(&self, other: &Self) -> bool {
        let mut pending = vec![(self, other)];
        while let Some((left, right)) = pending.pop() {
            if left.kind != right.kind
                || left.delimiter != right.delimiter
                || left.reader_prefixes != right.reader_prefixes
                || left.span != right.span
                || left.content_span != right.content_span
                || left.text != right.text
                || left.symbol_offset != right.symbol_offset
                || left.children.len() != right.children.len()
            {
                return false;
            }
            pending.extend(left.children.iter().zip(&right.children));
        }
        true
    }
}

impl Eq for ExpressionView {}

impl fmt::Debug for ExpressionView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        enum Action<'a> {
            View(&'a ExpressionView),
            Separator,
            Close(usize),
        }

        let mut actions = vec![Action::View(self)];
        while let Some(action) = actions.pop() {
            match action {
                Action::Separator => formatter.write_str(", ")?,
                Action::Close(symbol_offset) => {
                    write!(formatter, "], symbol_offset: {symbol_offset} }}")?;
                }
                Action::View(view) => {
                    write!(
                        formatter,
                        "ExpressionView {{ kind: {:?}, delimiter: {:?}, reader_prefixes: {:?}, span: {:?}, content_span: {:?}, text: {:?}, children: [",
                        view.kind,
                        view.delimiter,
                        view.reader_prefixes,
                        view.span,
                        view.content_span,
                        view.text,
                    )?;
                    actions.push(Action::Close(view.symbol_offset));
                    for (position, child) in view.children.iter().enumerate().rev() {
                        actions.push(Action::View(child));
                        if position > 0 {
                            actions.push(Action::Separator);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Drop for ExpressionView {
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.children);
        while let Some(mut view) = pending.pop() {
            pending.append(&mut view.children);
        }
    }
}

/// A validated selection of one non-root expression inside a syntax tree.
#[derive(Debug, Clone, Copy)]
pub struct Selection<'a> {
    pub(in crate::sexpr) tree: &'a SyntaxTree,
    pub(in crate::sexpr) node_id: NodeId,
}

/// The most [`SyntaxTree::find_parse_errors`] reports for one document,
/// however many syntax problems it actually has.
const MAX_RECOVERED_ERRORS: usize = 50;

/// The byte offset of the next line in `remaining` that starts, at column
/// zero, with an opening parenthesis — after byte `after`, which is where
/// the previous attempt failed.
///
/// Deliberately column-zero only, not "next non-whitespace after a
/// newline": a `(` indented under the form that just failed to parse is
/// still inside the broken structure, and resuming there would re-parse a
/// fragment of the same problem as if it were a fresh one. Requiring column
/// zero trades recall - a heavily-indented or non-conventional layout may
/// hide a real form boundary from this heuristic - for precision: it only
/// ever resyncs on what is unambiguously a new top-level form in
/// conventionally formatted Lisp source, which is what this tool's own
/// `edit format` produces and what virtually all Lisp source in the wild
/// looks like.
fn next_top_level_form_start(remaining: &str, after: usize) -> Option<usize> {
    let bytes = remaining.as_bytes();
    let mut search_from = after;
    loop {
        let newline_offset = remaining[search_from..].find('\n')?;
        let line_start = search_from + newline_offset + 1;
        if line_start >= bytes.len() {
            return None;
        }
        if bytes[line_start] == b'(' {
            return Some(line_start);
        }
        search_from = line_start;
    }
}

impl SyntaxTree {
    /// Append only the closing delimiters needed to balance unclosed lists.
    /// Refuses every other parser error so callers never guess at malformed input.
    pub fn repair_unclosed_lists(input: &str) -> Result<String, ParseError> {
        Parser::new(input).repair_unclosed_lists()
    }

    /// Parses source text into a syntax tree that preserves byte spans.
    pub fn parse(input: &str) -> std::result::Result<Self, ParseError> {
        let mut parser = Parser::new(input);
        parser.parse()
    }

    /// Parses source using the lexical and reader-macro rules of `dialect`.
    ///
    /// [`Self::parse`] intentionally retains the historical permissive reader;
    /// callers that know the file dialect should use this entry point.
    pub fn parse_with_dialect(
        input: &str,
        dialect: Dialect,
    ) -> std::result::Result<Self, ParseError> {
        let mut parser = Parser::with_dialect(input, dialect);
        parser.parse()
    }

    /// Every parse failure in `input`, instead of only the first.
    ///
    /// [`Self::parse_with_dialect`] stops at the first malformed form, so a
    /// document with three unrelated syntax problems costs three round
    /// trips to see: fix one, re-run, hit the next. This recovers by
    /// looking, after each failure, for the next line that starts at column
    /// zero with `(` — the shape essentially every top-level form in
    /// formatted Lisp source has — and re-parsing from there, so every
    /// problem in the document is visible in one pass. Bounded at
    /// [`MAX_RECOVERED_ERRORS`] so a pathological input (a syntax error on
    /// every line) does a bounded amount of work and produces a bounded
    /// report rather than one entry per line of a huge file.
    ///
    /// Returns no tree, on purpose. Recovery works by re-lexing independent
    /// suffixes of the document; splicing their partial trees into one
    /// coherent tree is not attempted, because the byte spans and node
    /// identities a caller would then act on could not be trusted the way
    /// [`Self::parse_with_dialect`]'s can. What can be trusted is "these are
    /// the places parsing broke" — which is what a caller fixing syntax
    /// errors before ever building a tree actually needs. A clean parse
    /// returns an empty vector, with the same result [`Self::parse_with_dialect`]
    /// would have returned `Ok` for.
    ///
    /// The column-zero heuristic means single-line or minified source, where
    /// no such line exists, falls back to exactly [`Self::parse_with_dialect`]'s
    /// behaviour: one error, the first one.
    #[must_use]
    pub fn find_parse_errors(input: &str, dialect: Dialect) -> Vec<ParseError> {
        let mut errors = Vec::new();
        let mut offset = 0usize;
        while offset < input.len() && errors.len() < MAX_RECOVERED_ERRORS {
            let remaining = &input[offset..];
            let Err(error) = Self::parse_with_dialect(remaining, dialect) else {
                break;
            };
            let resync = next_top_level_form_start(remaining, error.position());
            errors.push(error.shifted(offset));
            let Some(resync) = resync else {
                break;
            };
            offset += resync;
        }
        errors
    }

    /// Returns the exact source text this tree was parsed from.
    ///
    /// Analyses that build side tables keyed by node identity (spans, kinds)
    /// need the original bytes to slice or hash against, but only ever
    /// receive `&SyntaxTree` -- this is the only way back to them.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the direct children of the virtual root document node.
    #[must_use]
    pub fn root_children(&self) -> &[NodeId] {
        &self.node(NodeId::ROOT).children
    }

    /// Returns the span of the top-level form at `index`, or `None` past the
    /// end of the document.
    ///
    /// The allocation-free counterpart of
    /// `select_path(&Path::root_child(index))?.span()`. That spelling reads
    /// like a node-id lookup and a span read, and several callers document
    /// themselves as costing exactly that — but [`ExpressionPath`] owns a
    /// `Vec<ChildIndex>`, so `Path::root_child` heap-allocates, once per call.
    /// Callers that binary-search the top level pay `log2(forms)` allocations
    /// to answer one question about one node, which is what this exists to
    /// avoid; here it is an index into a slice and a field read.
    ///
    /// [`ExpressionPath`]: crate::sexpr::Path
    #[must_use]
    pub fn root_child_span(&self, index: usize) -> Option<ByteSpan> {
        self.root_children()
            .get(index)
            .map(|node_id| self.node(*node_id).span)
    }

    /// Returns an immutable tree view rooted at the virtual document node.
    #[must_use]
    pub fn root_view(&self) -> ExpressionView {
        self.expression_view(NodeId::ROOT)
    }

    /// Builds an outline of root-level lists and marks definition-like forms.
    pub fn outline(&self, is_definition_head: impl Fn(&str) -> bool) -> Vec<OutlineEntry> {
        self.node(NodeId::ROOT)
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, node_id)| {
                let node = self.node(*node_id);
                if node.kind != NodeKind::List {
                    return None;
                }
                let head = node
                    .children
                    .first()
                    .and_then(|child| self.atom_text(*child))
                    .map(ToOwned::to_owned);
                let definition_like = head.as_deref().is_some_and(&is_definition_head);
                Some(OutlineEntry {
                    path: ExpressionPath::root_child(index),
                    span: node.span,
                    head,
                    definition_like,
                })
            })
            .collect()
    }

    /// Reports whether any comment discovered during parsing falls within
    /// `span`. Callers that rebuild source text from parsed atoms (rather
    /// than slicing it verbatim) can use this to detect when doing so would
    /// silently discard a comment, since comments live outside the node tree
    /// and are otherwise invisible to such callers.
    #[must_use]
    pub fn has_comment_in(&self, span: ByteSpan) -> bool {
        self.comments
            .iter()
            .any(|comment| comment.span.start() < span.end() && span.start() < comment.span.end())
    }

    /// Every comment the parse found, in source order.
    ///
    /// Comments are kept outside the node tree, so a report that reads them —
    /// an Emacs Lisp autoload cookie, a `TODO` marker, a suppression
    /// directive — cannot get at them by walking `children`. Exposing the
    /// parser's own record is the only way to ask "is this text a comment?"
    /// without re-lexing the file and getting string literals wrong.
    pub fn comments(&self) -> impl Iterator<Item = SourceComment<'_>> {
        self.comments.iter().map(|comment| SourceComment {
            span: comment.span,
            text: comment.text.as_str(),
            own_line: comment.own_line,
        })
    }

    /// Collects every atom in the tree together with its path and byte span.
    #[must_use]
    pub fn atom_occurrences(&self) -> Vec<AtomOccurrence> {
        self.collect_atom_occurrences(false)
    }

    /// Counts the atoms `atom_occurrences` would report without materializing
    /// their paths and text. Callers that only need the total (e.g. workspace
    /// inventory reports) avoid one `String` and one path `Vec` per atom.
    #[must_use]
    pub fn atom_occurrence_count(&self) -> usize {
        let mut count: usize = 0;
        let mut pending = self.node(NodeId::ROOT).children.clone();
        while let Some(node_id) = pending.pop() {
            let node = self.node(node_id);
            if node.opaque_reader_form
                || node
                    .reader_prefixes()
                    .iter()
                    .any(|prefix| prefix.is_opaque_reader_form())
            {
                continue;
            }
            if node.kind == NodeKind::Atom {
                let is_quoted_literal = node.reader_prefixes().contains(&ReaderPrefix::Quote);
                count = count.saturating_add(usize::from(
                    !is_quoted_literal
                        && node
                            .span
                            .slice(&self.source)
                            .get(node.symbol_offset as usize..)
                            .is_some(),
                ));
                continue;
            }
            pending.extend(node.children.iter().copied());
        }
        count
    }

    #[must_use]
    pub fn atom_occurrence_index(&self) -> AtomOccurrenceIndex<'_> {
        let mut parent_steps = vec![None; self.nodes.len()];
        let mut occurrences = Vec::new();
        let mut quoted_designators = Vec::new();
        let mut pending = self
            .node(NodeId::ROOT)
            .children
            .iter()
            .copied()
            .enumerate()
            .rev()
            .map(|(index, node_id)| (node_id, NodeId::ROOT, index))
            .collect::<Vec<_>>();

        while let Some((node_id, parent_id, index)) = pending.pop() {
            parent_steps[node_id.get()] = Some((parent_id, index));
            let node = self.node(node_id);
            if node.opaque_reader_form
                || node
                    .reader_prefixes()
                    .iter()
                    .any(|prefix| prefix.is_opaque_reader_form())
            {
                continue;
            }
            if node.kind == NodeKind::Atom {
                if let Some(text) = node
                    .span
                    .slice(&self.source)
                    .get(node.symbol_offset as usize..)
                {
                    let occurrence = BorrowedAtomOccurrence {
                        node_id,
                        span: ByteSpan::new(
                            ByteOffset::new(node.span.start().get() + node.symbol_offset as usize),
                            node.span.end(),
                        ),
                        text,
                    };
                    if node.reader_prefixes().contains(&ReaderPrefix::Quote) {
                        quoted_designators.push(occurrence);
                    } else {
                        occurrences.push(occurrence);
                    }
                }
                continue;
            }
            pending.extend(
                node.children
                    .iter()
                    .copied()
                    .enumerate()
                    .rev()
                    .map(|(index, child_id)| (child_id, node_id, index)),
            );
        }
        debug_assert!(occurrences.windows(2).all(|pair| {
            let left = (pair[0].span.start(), pair[0].span.end());
            let right = (pair[1].span.start(), pair[1].span.end());
            left < right
        }));
        AtomOccurrenceIndex {
            parent_steps,
            occurrences,
            quoted_designators,
        }
    }

    /// Collects bare quoted-symbol designators (`'foo`, i.e. an atom whose own
    /// reader prefix is `'`), which `atom_occurrences` deliberately treats as
    /// inert data and excludes (see `does_not_rename_quoted_atom_occurrences`).
    ///
    /// That exclusion is right for `atom_occurrences`'s other consumers
    /// (unused-definition/impact/analysis reports, which have their own,
    /// more precise quote-aware reference collectors when they need one), but
    /// `'foo` is also the standard Common Lisp idiom for referencing a symbol
    /// in the value/type namespace as data -- e.g. `(error 'foo ...)`,
    /// `(typep x 'foo)`, `(make-instance 'foo)`. A blunt, tree-wide rename
    /// (`rename-symbol`, `refactor preview --mode symbol`) that skips these
    /// would silently leave behind references to a definition that no longer
    /// exists, so those two entry points additionally consult this method.
    ///
    /// Only a *bare* quoted atom counts: a quoted list such as `'(foo bar)`
    /// keeps its reader prefix on the list node, not on `foo`/`bar`, so those
    /// remain ordinary atoms already covered by `atom_occurrences` and are
    /// left untouched here.
    #[must_use]
    pub fn quoted_symbol_designator_occurrences(&self) -> Vec<AtomOccurrence> {
        self.collect_atom_occurrences(true)
    }

    /// Rewrites matching atom occurrences while preserving the rest of the source text.
    ///
    /// # Examples
    ///
    /// ```
    /// use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};
    ///
    /// let input = "(let ((value 1)) (+ value value))";
    /// let tree = SyntaxTree::parse(input).unwrap();
    /// let output = tree.rename_symbol(
    ///     &SymbolName::new("value").unwrap(),
    ///     &SymbolName::new("count").unwrap(),
    /// );
    ///
    /// assert_eq!(output, "(let ((count 1)) (+ count count))");
    /// ```
    #[must_use]
    pub fn rename_symbol(&self, from: &SymbolName, to: &SymbolName) -> String {
        let input = self.source.as_str();
        let index = self.atom_occurrence_index();
        let mut occurrences = index
            .rename_occurrences()
            .filter(|occurrence| common_lisp_symbol_reference_eq(occurrence.text, from.as_str()))
            .collect::<Vec<_>>();
        occurrences.sort_by_key(|occurrence| occurrence.span.start());
        let mut output = String::with_capacity(input.len());
        let mut cursor = 0usize;
        for occurrence in occurrences {
            let range = occurrence.span.as_range();
            if range.start < cursor {
                continue;
            }
            output.push_str(&input[cursor..range.start]);
            output.push_str(to.as_str());
            cursor = range.end;
        }
        output.push_str(&input[cursor..]);
        output
    }

    /// Resolves a zero-based expression path into a non-root selection.
    pub fn select_path(&self, path: &ExpressionPath) -> SexprResult<Selection<'_>> {
        let mut node_id = NodeId::ROOT;
        for (depth, index) in path.indexes().iter().enumerate() {
            let node = self.node(node_id);
            node_id = *node.children.get(index.get()).ok_or_else(|| {
                let resolved = path.indexes()[..depth]
                    .iter()
                    .map(|resolved| resolved.get().to_string())
                    .collect::<Vec<_>>()
                    .join(".");
                let location = if resolved.is_empty() {
                    "the top level".to_owned()
                } else {
                    format!("the form at path {resolved}")
                };
                let arity = match node.children.len() {
                    0 => format!("{location} has no child expressions"),
                    len => format!(
                        "{location} has {len} child expressions (valid indexes 0..={})",
                        len.saturating_sub(1)
                    ),
                };
                SelectionError::PathSegmentOutOfRange {
                    segment: index.get(),
                    detail: arity,
                }
            })?;
        }
        if node_id == NodeId::ROOT {
            return Err(StructureError::RootNotEditable.into());
        }
        Ok(Selection {
            tree: self,
            node_id,
        })
    }

    /// Selects the smallest expression that contains the given byte offset.
    pub fn select_at(&self, offset: usize) -> SexprResult<Selection<'_>> {
        let not_found = || SexprError::from(SelectionError::NoExpressionAtOffset { offset });
        // The offset is a caller's argument, not a parser product, so it is
        // not bounded by the document's length. Past the end of the source it
        // is inside no node and the search below would fail anyway; rejecting
        // it here also keeps it away from `ByteOffset::new`, which panics
        // rather than truncate above four gigabytes.
        if offset > self.source.len() {
            return Err(not_found());
        }
        let offset = ByteOffset::new(offset);
        let mut best = None;
        for id in 1..self.nodes.len() {
            let node_id = NodeId::new(id);
            let node = self.node(node_id);
            if node.span.contains(offset) {
                match best {
                    None => best = Some(node_id),
                    Some(best_id) if node.span.len() < self.node(best_id).span.len() => {
                        best = Some(node_id);
                    }
                    _ => {}
                }
            }
        }
        best.map(|node_id| Selection {
            tree: self,
            node_id,
        })
        .ok_or_else(not_found)
    }

    // A full `ExpressionPath` is only built when an atom is found. Enter/leave
    // frames preserve pre-order traversal without consuming the call stack.
    fn collect_atom_occurrences(&self, quoted_designators: bool) -> Vec<AtomOccurrence> {
        enum Frame {
            Enter { node_id: NodeId, index: usize },
            Leave,
        }

        let mut output = Vec::new();
        let mut path_stack = Vec::new();
        let mut pending = self
            .node(NodeId::ROOT)
            .children
            .iter()
            .copied()
            .enumerate()
            .rev()
            .map(|(index, node_id)| Frame::Enter { node_id, index })
            .collect::<Vec<_>>();

        while let Some(frame) = pending.pop() {
            let Frame::Enter { node_id, index } = frame else {
                path_stack.pop();
                continue;
            };
            path_stack.push(index);
            pending.push(Frame::Leave);

            let node = self.node(node_id);
            if node.opaque_reader_form
                || node
                    .reader_prefixes()
                    .iter()
                    .any(|prefix| prefix.is_opaque_reader_form())
            {
                continue;
            }
            if node.kind == NodeKind::Atom {
                let is_quoted_literal = node.reader_prefixes().contains(&ReaderPrefix::Quote);
                if is_quoted_literal == quoted_designators {
                    if let Some(symbol_text) = node
                        .span
                        .slice(&self.source)
                        .get(node.symbol_offset as usize..)
                    {
                        let symbol_span = ByteSpan::new(
                            ByteOffset::new(node.span.start().get() + node.symbol_offset as usize),
                            node.span.end(),
                        );
                        output.push(AtomOccurrence {
                            path: ExpressionPath::from_indexes(path_stack.clone()),
                            span: symbol_span,
                            text: symbol_text.to_string(),
                        });
                    }
                }
                continue;
            }
            pending.extend(
                node.children
                    .iter()
                    .copied()
                    .enumerate()
                    .rev()
                    .map(|(index, node_id)| Frame::Enter { node_id, index }),
            );
        }
        output
    }

    fn atom_text(&self, node_id: NodeId) -> Option<&str> {
        let node = self.node(node_id);
        if node.kind != NodeKind::Atom
            || node.opaque_reader_form
            || !node.reader_prefixes().is_empty()
        {
            return None;
        }
        Some(node.span.slice(&self.source))
    }

    pub(in crate::sexpr) fn expression_view(&self, node_id: NodeId) -> ExpressionView {
        let mut frames = vec![(node_id, false)];
        let mut views = Vec::new();

        while let Some((current_id, expanded)) = frames.pop() {
            let node = self.node(current_id);
            if !expanded {
                frames.push((current_id, true));
                frames.extend(node.children.iter().rev().map(|child| (*child, false)));
                continue;
            }

            let children_start = views
                .len()
                .checked_sub(node.children.len())
                .expect("expanded expression has all child views");
            let children = views.split_off(children_start);
            views.push(ExpressionView {
                kind: match node.kind {
                    NodeKind::Root => ExpressionKind::Root,
                    NodeKind::List => ExpressionKind::List,
                    NodeKind::Atom => ExpressionKind::Atom,
                },
                delimiter: node.delimiter,
                reader_prefixes: node.reader_prefixes().to_vec(),
                span: node.span,
                content_span: ByteSpan::new(
                    match node.kind {
                        NodeKind::List => node.open.unwrap_or(node.span.start()),
                        NodeKind::Atom => {
                            ByteOffset::new(node.span.start().get() + node.symbol_offset as usize)
                        }
                        NodeKind::Root => node.span.start(),
                    },
                    node.span.end(),
                ),
                text: (node.kind == NodeKind::Atom)
                    .then(|| node.span.slice(&self.source).to_string()),
                symbol_offset: node.symbol_offset as usize,
                children,
            });
        }

        views
            .pop()
            .expect("expression view root is always constructed")
    }

    pub(in crate::sexpr) fn node(&self, node_id: NodeId) -> &Node {
        &self.nodes[node_id.get()]
    }
}

impl<'a> Selection<'a> {
    pub fn validate_source(self, input: &str) -> Result<(), SelectionError> {
        if self.tree.source != input {
            return Err(SelectionError::SourceMismatch);
        }
        self.span()
            .validate_against(input)
            .map_err(|source| SelectionError::InvalidSpan { source })
    }

    pub fn validate_context(self, input: &str, tree: &SyntaxTree) -> Result<(), SelectionError> {
        self.validate_tree(tree)?;
        self.validate_source(input)
    }

    pub fn validate_tree(self, tree: &SyntaxTree) -> Result<(), SelectionError> {
        if !std::ptr::eq(tree, self.tree) {
            return Err(SelectionError::TreeMismatch);
        }
        Ok(())
    }

    /// Returns the original source text covered by this selection.
    #[must_use]
    pub fn text(self) -> &'a str {
        self.span().slice(&self.tree.source)
    }

    pub(in crate::sexpr) fn node(self) -> &'a Node {
        self.tree.node(self.node_id)
    }

    /// Returns the byte span of the selected expression.
    #[must_use]
    pub fn span(self) -> ByteSpan {
        self.node().span
    }

    /// Returns an immutable view of the selected expression subtree.
    #[must_use]
    pub fn view(self) -> ExpressionView {
        self.tree.expression_view(self.node_id)
    }

    /// Returns the zero-based path from the virtual root to this selection.
    ///
    /// The inverse of [`SyntaxTree::select_path`]. Every command in this tool
    /// addresses a form by path, so an operation that *finds* a form — `select
    /// --at`, `edit navigate` — has to be able to say which path it landed on,
    /// or its answer cannot be fed back in.
    #[must_use]
    pub fn path(self) -> ExpressionPath {
        let mut indexes = Vec::new();
        let mut current = self.node_id;
        while let Some(parent_id) = self.tree.node(current).parent {
            let position = self
                .tree
                .node(parent_id)
                .children
                .iter()
                .position(|id| *id == current)
                .expect("a node is listed among its own parent's children");
            indexes.push(position);
            current = parent_id;
        }
        indexes.reverse();
        ExpressionPath::from_indexes(indexes)
    }

    /// Returns whether this selection is a list or an atom.
    #[must_use]
    pub fn kind(self) -> ExpressionKind {
        match self.node().kind {
            NodeKind::Root => ExpressionKind::Root,
            NodeKind::List => ExpressionKind::List,
            NodeKind::Atom => ExpressionKind::Atom,
        }
    }

    /// Returns the head symbol of a selected list, ignoring reader prefixes.
    #[must_use]
    pub fn head(self) -> Option<&'a str> {
        let node = self.node();
        if node.kind != NodeKind::List {
            return None;
        }
        self.tree.atom_text(*node.children.first()?)
    }

    /// Returns the enclosing list span when the parent node is a list.
    pub fn enclosing_list_span(self) -> SexprResult<ByteSpan> {
        let parent_id = self.node().parent.ok_or(StructureError::NoEnclosingList)?;
        let parent = self.tree.node(parent_id);
        if parent.kind != NodeKind::List {
            return Err(StructureError::NoEnclosingList.into());
        }
        Ok(parent.span)
    }
}
