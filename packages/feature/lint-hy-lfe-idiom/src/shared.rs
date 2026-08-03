//! What the rules in this crate share: how to read an atom, how to tell code
//! from data, and the small amount of Hy and LFE surface syntax the rules
//! agree on.
//!
//! Nothing here runs per visited node. Every helper is called from inside a
//! rule that has already matched its head, and the expensive one
//! ([`is_unevaluated_at`]) is called only once a finding is otherwise ready to
//! report — it descends from [`SyntaxTree::root_view`], which materializes the
//! whole document, so calling it before the cheap head/shape check would cost
//! four orders of magnitude more per invocation than the check it guards.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};

/// An atom's text, exactly as the source spells it — *including* any reader
/// prefix.
///
/// That is what makes this safe to compare against a bare symbol name without
/// also testing `reader_prefixes`: `'defun` has `text == "'defun"`, which is
/// not equal to `"defun"`, so quoted data can never be read as an operator.
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The exact head symbol of a `(...)` list.
///
/// Paren-delimited only. In Hy a `[...]` is a list literal and a `{...}` is a
/// dict literal — neither is a call — so reading a "head" off one would invent
/// an operator the reader never produced.
///
/// The delimiter test is, today, redundant with the dispatcher: the engine
/// keys its head index off `paredit_core_syntax::view_query::list_head`, which
/// is itself paren-only, so a `[is 1 2]` never reaches a rule in the first
/// place. Mutation-testing confirmed it — relaxing this to accept any list
/// killed no test.
///
/// It stays for the reason the head index states about itself: it is a
/// *pre-filter* that "must never be narrower than any rule's notion of the
/// same operator, but may be wider". A rule that leaned on the index for its
/// notion of what a call is would be correct only by accident of the
/// dispatcher's current shape.
pub(crate) fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren))
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

/// Whether `view` is a list with the given delimiter and no reader prefix.
///
/// The prefix test is the whole point: `#()` in Hy is an immutable *tuple*
/// and `()` is a call, both of which are paren lists, and `#{}` is a mutable
/// set while `{}` is a mutable dict. A delimiter alone does not identify a
/// literal in either language.
pub(crate) fn is_plain_list(view: &ExpressionView, delimiter: Delimiter) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(delimiter)
        && view.reader_prefixes.is_empty()
}

/// Whether `view` is a `#`-prefixed list with the given delimiter: Hy's
/// `#(...)` tuple and `#{...}` set.
pub(crate) fn is_hash_list(view: &ExpressionView, delimiter: Delimiter) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(delimiter)
        && view.reader_prefixes.contains(&ReaderPrefix::HashLiteral)
}

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down. A single `i32` depth counter cannot express that and has shipped
/// elsewhere in this workspace as a false-positive source.
///
/// **This models LFE exactly and Hy only in the safe direction.** LFE spells
/// unquote `,` and `,@`, which the reader turns into
/// [`ReaderPrefix::Unquote`]/[`ReaderPrefix::UnquoteSplicing`], so `quasi`
/// counts down as it should. Hy spells unquote `~` and `~@`, which this
/// workspace's reader does *not* recognize as prefixes at all — it emits a
/// bare `~` atom followed by the form. `quasi` therefore never counts down for
/// Hy, so everything textually inside a Hy `` ` `` template reads as data.
/// That direction suppresses findings rather than inventing them, which is the
/// side to be wrong on; it is recorded in this crate's README as a known
/// limitation rather than worked around here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#`, `#'` and the rest are deliberately neutral: none of them turns
    /// code into data. `#` in particular must stay neutral, because it is how
    /// both languages spell a *literal collection* — an LFE `#(tuple)` and a
    /// Hy `#{set}` are values, but the forms inside them are still read, and
    /// treating the prefix as a quote would silence every rule under one.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// The long-hand `(quote …)`, which the reader also produces for `'…` but
/// which hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view) == Some("quote")
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
/// The root view it starts from is what makes this expensive relative to a
/// shape test, so every caller applies its own cheap check first.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor
/// being data does not settle it: `` `(a ,(catch (f))) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// Delegates to [`node_context`] rather than repeating the descent. The two
/// were separate walks at first, and mutation-testing found the duplication
/// immediately: breaking the long-hand `(quote …)` handling in one copy killed
/// no test, because the rules that had a test for it were reading the *other*
/// copy.
#[must_use]
pub(crate) fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    node_context(tree, target).is_data
}

/// Everything one root-to-target descent can answer at once.
#[derive(Debug, Clone, Default)]
pub(crate) struct NodeContext {
    /// Whether the node is unevaluated data rather than code.
    pub(crate) is_data: bool,
    /// The head of the node that *directly* encloses the target, or `None`
    /// when it is a top-level form or its parent is not a paren list.
    pub(crate) parent_head: Option<String>,
}

/// Both facts about `target`, from a single descent.
///
/// This exists for cost, not tidiness. [`SyntaxTree::root_view`] materializes
/// the whole document into owned views, so its cost is the file's size, not
/// the node's depth; a rule that called [`is_unevaluated_at`] and then asked
/// separately for the enclosing head paid that twice. Measured on a 14840-byte
/// LFE fixture, `lfe-catch-swallows-exit` with two descents cost 1576174
/// ns/call against 894237 on the half-size fixture — merging them is the
/// single largest saving available without changing the core API.
///
/// The remaining per-finding cost is proportional to the document, which is a
/// property of `root_view` shared with the shipped `elisp-hook-lambda` (also
/// measured: 239219 ns/call at one size, 459777 at double). Fixing that
/// belongs in `core/syntax`, not here.
///
/// This is why every caller applies its own cheap head and shape checks first
/// and reaches this only with a finding otherwise ready to report.
///
/// A span that names no node is judged by the innermost node that contains it,
/// which is the honest answer for a span a caller synthesized rather than took
/// from the tree.
#[must_use]
pub(crate) fn node_context(tree: &SyntaxTree, target: ByteSpan) -> NodeContext {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;
    let mut parent_head: Option<String> = None;

    loop {
        let quoting = is_quote_form(view);
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return NodeContext {
                is_data: state.is_data(),
                parent_head,
            };
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        parent_head = list_head(view).map(ToOwned::to_owned);
        view = child;
        if view.span == target {
            return NodeContext {
                is_data: state.is_data(),
                parent_head,
            };
        }
    }
}
