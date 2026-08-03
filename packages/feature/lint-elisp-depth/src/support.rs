//! What the rules in this crate share: reading an atom, and reading a node's
//! *position* in the tree.
//!
//! Nothing here runs per visited node. Every helper is called from a rule that
//! has already matched its head, and the one expensive helper
//! ([`ancestry_at`]) descends from the root, so it is called only after the
//! cheap head comparison has already said the node is in the rule's domain.
//! Getting that order wrong is not a style point: a sibling package measured
//! 450843 ns/call against 28 ns/call purely from reaching the root before the
//! head check, and the `clean/forms/*` benchmarks lint files with zero
//! findings, so the cost of a rule that matches nothing is exactly what they
//! measure.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix};

/// An atom's text, exactly as the source spells it — *including* any reader
/// prefix.
///
/// That is what makes this safe to compare against a bare symbol name without
/// also testing `reader_prefixes`: `#'insert` has `text == "#'insert"`, which
/// is not equal to `"insert"`, so a function designator is never read as a
/// call head.
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// An atom's *symbol* text, past any reader prefix.
pub(crate) fn prefixed_atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
        .and_then(|text| text.get(view.symbol_offset..))
}

/// The exact head symbol of a `(...)` list.
///
/// `head_key` case-folds **only** for `Dialect::CommonLisp`; an Emacs Lisp head
/// is indexed exactly as written, so this compares byte-exactly and so must
/// every caller's `Heads` entry.
pub(crate) fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List)
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

/// Whether `view` is a `lambda` form, in any spelling that evaluates to a
/// function.
///
/// A quote-prefixed `'(lambda …)` answers `false`: it is a list, not a
/// function, and `elisp-quoted-lambda` in the `emacs-lisp` package already
/// makes a strictly stronger complaint about it.
pub(crate) fn is_lambda_form(view: &ExpressionView) -> bool {
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return false;
    }
    match view.kind {
        ExpressionKind::List => match list_head(view) {
            Some("lambda") => true,
            // `(function (lambda …))`, the long hand of `#'(lambda …)`.
            Some("function") => view
                .children
                .get(1)
                .is_some_and(|inner| list_head(inner) == Some("lambda")),
            _ => false,
        },
        _ => false,
    }
}

/// The `lambda` inside `#'(lambda …)`/`(function (lambda …))`, or the form
/// itself when it is already a bare `lambda`.
pub(crate) fn lambda_form(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_lambda_form(view) {
        return None;
    }
    match list_head(view) {
        Some("function") => view.children.get(1),
        _ => Some(view),
    }
}

/// The value that follows `keyword` in a plist-style argument tail.
///
/// `make-process` takes `:name x :filter f …`; the value is the child after
/// the keyword atom. Scanning in steps of two from the first keyword would
/// mis-align on a malformed call, so this simply looks for the keyword and
/// takes its successor.
pub(crate) fn keyword_value<'a>(
    view: &'a ExpressionView,
    keyword: &str,
) -> Option<&'a ExpressionView> {
    let children = view.children.get(1..)?;
    children
        .iter()
        .position(|child| atom_text(child) == Some(keyword))
        .and_then(|index| children.get(index + 1))
}

/// Whether any node in `view`'s subtree satisfies `predicate`.
///
/// Bounded by the subtree, not the file: every caller passes a single lambda
/// body it has already isolated.
pub(crate) fn subtree_any(
    view: &ExpressionView,
    predicate: &mut impl FnMut(&ExpressionView) -> bool,
) -> bool {
    if predicate(view) {
        return true;
    }
    view.children
        .iter()
        .any(|child| subtree_any(child, predicate))
}

// ---------------------------------------------------------------------------
// Evaluation context and position
// ---------------------------------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down. A single `i32` depth counter cannot express that and has shipped
/// elsewhere in this workspace as a false-positive source.
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

fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view) == Some("quote")
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Where a node sits: whether it is data, and what its parent is.
///
/// Both rules in this crate need a slice of this, and both would otherwise
/// walk the tree from the root separately. One descent answers them.
pub(crate) struct Ancestry<'a> {
    /// Whether the node is unevaluated data rather than code.
    ///
    /// Read *at* the node and nowhere shallower: `` `(a ,(run-with-timer 1 1 #'f)) ``
    /// has a quasiquoted ancestor and an evaluated target.
    pub(crate) is_data: bool,
    /// The node's immediate parent, or `None` when the node is a top-level
    /// form.
    pub(crate) parent: Option<&'a ExpressionView>,
    /// The node's index among its parent's children.
    pub(crate) index_in_parent: usize,
}

impl Ancestry<'_> {
    /// Whether the node's value flows somewhere a later caller could use it.
    ///
    /// True when it is the last child of its parent — the value of an implicit
    /// `progn`, the value bound by a `let` pair `(tm (run-with-timer …))`, the
    /// value of the whole enclosing form — or when the parent is a form that
    /// consumes its argument's value. A top-level form (`parent == None`)
    /// answers **false**: nothing reads the value of a form at file scope.
    pub(crate) fn value_is_used(&self, consuming_heads: &[&str]) -> bool {
        let Some(parent) = self.parent else {
            return false;
        };
        if self.index_in_parent + 1 == parent.children.len() {
            return true;
        }
        list_head(parent).is_some_and(|head| consuming_heads.contains(&head))
    }
}

/// Descend from `root` to the node whose span is `target`, recording what
/// encloses it.
///
/// Cost is the node's depth, not the file's size. **Call this only after the
/// rule's cheap head comparison has matched** — see the module docstring.
pub(crate) fn ancestry_at<'a>(root: &'a ExpressionView, target: ByteSpan) -> Ancestry<'a> {
    let mut view: &'a ExpressionView = root;
    let mut state = QuoteState::EVALUATED;
    let mut parent: Option<&'a ExpressionView> = None;
    let mut index_in_parent = 0usize;

    loop {
        let quoting = is_quote_form(view);
        // A span naming no node is judged by the innermost node containing it,
        // which is the honest answer for a span a caller synthesized.
        let Some((index, child)) = view
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| span_contains(child.span, target))
        else {
            return Ancestry {
                is_data: state.is_data(),
                parent,
                index_in_parent,
            };
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        // The root view is a synthetic container for the file's top-level
        // forms, not a form itself, so a top-level node reports no parent.
        parent = (view.kind == ExpressionKind::List).then_some(view);
        index_in_parent = index;
        view = child;
        if view.span == target {
            return Ancestry {
                is_data: state.is_data(),
                parent,
                index_in_parent,
            };
        }
    }
}
