//! Shared helpers for this package's rules: how to read a Hy form, and how to
//! tell code from data.
//!
//! Three facts about Hy drive everything here, each verified against this
//! workspace's own reader rather than assumed:
//!
//! - **Only `(...)` is a call.** In Hy a `[...]` is a list literal, a `{...}`
//!   is a dict literal, `#(...)` is a tuple and `#{...}` is a set. Reading a
//!   "head" off any of them would invent an operator the reader never
//!   produced. This is the opposite of Racket, where `[...]` is an ordinary
//!   form spelling, so the helper cannot be shared between the two.
//! - **Hy is case sensitive**, because Python is, and the head index agrees:
//!   `head_key` returns the head *verbatim* for every dialect except Common
//!   Lisp (`head_index.rs:81-86`), so `(TRY …)` is keyed `TRY` and never
//!   reaches a rule registered for `try`. Every rule here still re-compares the
//!   head byte for byte through [`hy_head`], because the index documents itself
//!   as a pre-filter that "must never be *narrower* than any rule's notion of
//!   'same operator', but may be wider" — a rule that took the index's word for
//!   which operator it had would be correct only by accident of the
//!   dispatcher's current shape. Mutation testing confirms the position: with
//!   the byte-exact comparison deleted, no test in this package fails today.
//! - **`,` is an identifier constituent.** `NON_IDENT` in Hy's reader is
//!   ``set("()[]{};\"'`~")`` and does not list `,`, so `1,` is the single
//!   symbol `1,` and `(,)` is a call to a symbol named `,`. Nothing here may
//!   treat a comma as a separator.
//!
//! # Why the quote model cannot count down for Hy
//!
//! `QuoteState` is the two-counter model from
//! `packages/feature/lint-condition-system/src/support.rs` — `hard: bool` plus
//! `quasi: u32`. A single `i32` depth counter is wrong and has shipped in this
//! workspace as a false-positive source twice. A consolidation of the several
//! copies is in flight; this one should move when it lands.
//!
//! For Hy the model is **deliberately asymmetric**, and this is a reader
//! limitation rather than a choice made here. Hy spells quasiquote `` ` `` and
//! unquote `~`/`~@`. The reader recognizes the first as
//! [`ReaderPrefix::Quasiquote`], so `quasi` counts *up*; it does **not**
//! recognize `~` as [`ReaderPrefix::Unquote`], so `quasi` never counts *down*.
//! Verified by parsing `` `(setv ~name 5) `` with this workspace's reader: the
//! template is a `List` carrying `Quasiquote`, and `~name` arrives as a single
//! `Atom` whose text is `"~name"` — not as an unquote prefix on `name`.
//!
//! `reader_policy.rs` (`classify_hy`) documents why: making `~` a prefix was
//! implemented and measured, and is blocked on a formatter bug that drops a
//! child list's reader prefixes, which would silently rewrite `~x` to `x`
//! inside macro bodies.
//!
//! The consequence for every rule here is that **everything textually inside a
//! Hy `` ` `` reads as data**, so no rule fires inside a macro template. That
//! direction suppresses findings rather than inventing them, which is the side
//! to be wrong on.
//!
//! One thing that does *not* follow, and which is worth stating because the
//! sibling Carp package has the opposite problem: `~name` and `~@body` scan as
//! single atoms, so a `~` does **not** appear as an extra sibling inflating its
//! enclosing form's arity. Only `~` immediately before a delimiter — `~(f)` —
//! produces a bare `~` atom, because `(` terminates the identifier. Arity
//! counting inside a Hy template is therefore sound except across `~(`.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix,
    SyntaxTree,
};

/// An atom's text, exactly as the source spells it.
#[must_use]
pub fn hy_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// Whether `view` is a list written `(...)` with no reader prefix — a call.
///
/// The prefix test is load-bearing rather than decorative: `#(1 2)` is a
/// *tuple* and parses as a `List` with delimiter `Paren` carrying
/// [`ReaderPrefix::HashLiteral`], so the engine's head index offers it to every
/// rule registered for the head `1`. It is a self-evaluating constant, not a
/// form.
#[must_use]
pub fn is_call(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && view.reader_prefixes.is_empty()
}

/// Whether `view` is a plain `[...]` list literal, which is how Hy spells a
/// parameter list, a binding list and an `except` clause's binding list.
#[must_use]
pub fn is_bracket_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Bracket)
        && view.reader_prefixes.is_empty()
}

/// The head symbol of a Hy call, compared byte for byte by the caller.
///
/// Deliberately not [`paredit_core_syntax::view_query::symbol_is`], which folds
/// ASCII case and strips everything before a `:`. Hy is case sensitive, and `:`
/// is an ordinary identifier constituent — `foo:bar` is one name whose tail
/// must not be mistaken for the bare operator.
#[must_use]
pub fn hy_head(view: &ExpressionView) -> Option<&str> {
    if !is_call(view) {
        return None;
    }
    hy_atom(view.children.first()?)
}

/// Whether `view` is a call whose head is exactly `name`.
#[must_use]
pub fn heads_with(view: &ExpressionView, name: &str) -> bool {
    hy_head(view) == Some(name)
}

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing.
/// `hard` never clears, since `'` has no escape. `quasi` is a count so that a
/// nested template reaches code again at the right level rather than at the
/// first escape.
///
/// See this module's header for why `quasi` never comes back down for Hy.
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
    /// [`ReaderPrefix::HashLiteral`] enters data: Hy's `#(...)` tuple and
    /// `#{...}` set are self-evaluating constants, so a `(try ...)` written
    /// inside one is a value, not a statement.
    ///
    /// The `Unquote` arms are kept even though this workspace's Hy reader
    /// never emits them. They cost nothing, and they are what makes this
    /// module correct on the day `classify_hy` gains its two arms — at which
    /// point the header's asymmetry note becomes obsolete rather than wrong.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote | ReaderPrefix::HashLiteral => self.hard = true,
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

/// The long-hand quoting forms, which macro output spells out even when the
/// reader shorthand exists.
///
/// Hy really does bind these names: `(quote x)`, `(quasiquote x)`,
/// `(unquote x)` and `(unquote-splice x)` are what its reader macros expand to.
fn quoting_form(view: &ExpressionView) -> Option<QuoteKind> {
    match hy_head(view)? {
        "quote" => Some(QuoteKind::Hard),
        "quasiquote" => Some(QuoteKind::Quasi),
        "unquote" | "unquote-splice" => Some(QuoteKind::Unquote),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum QuoteKind {
    Hard,
    Quasi,
    Unquote,
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// What a node's ancestors say about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeContext {
    /// The node is unevaluated data rather than code.
    pub unevaluated: bool,
    /// The head of the form that directly encloses the node, or `None` when it
    /// is top level or its parent is not a call.
    pub parent_head: Option<String>,
}

/// The ancestor questions this package asks, answered in one descent.
///
/// # Cost
///
/// This is the expensive call in the package, and the shape of it is
/// deliberate because the obvious spelling is quadratic. A feature crate gets
/// no parent link and no borrowed node arena from [`SyntaxTree`]; the only way
/// in is [`SyntaxTree::root_view`], which *materializes the whole document* as
/// fresh [`ExpressionView`]s — a `Vec` per node and a `String` per atom. Calling
/// that once per candidate costs O(file) per candidate, so O(file²) per file. A
/// sibling package measured **450843 ns/call against 28 ns/call** purely from
/// calling this before its node-local checks instead of after them.
///
/// So the descent is rooted at the enclosing *top-level form*, found by binary
/// search over the root's children through [`SyntaxTree::root_child_span`] — a
/// slice index and a field read, with no heap allocation at all. Nothing
/// outside a top-level form can quote it or be its parent, so the answer is
/// identical.
///
/// Callers must still treat this as the last thing they do: every rule here
/// settles its head and shape questions first and reaches this only with a
/// finding otherwise ready to report, so a clean file — the overwhelmingly
/// common case — never pays for it at all.
#[must_use]
pub fn node_context(tree: &SyntaxTree, target: ByteSpan) -> NodeContext {
    let Some(form) = enclosing_top_level_form(tree, target) else {
        return NodeContext::default();
    };
    context_within(&form, target)
}

/// The index of the top-level form whose span contains `target`.
///
/// The root's children are disjoint and in source order, so the first one whose
/// span ends after `target` begins is the only candidate.
#[must_use]
pub fn enclosing_top_level_index(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
    let count = tree.root_children().len();
    let (mut low, mut high) = (0, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let span = tree.root_child_span(middle)?;
        if span.end().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low >= count {
        return None;
    }
    span_contains(tree.root_child_span(low)?, target).then_some(low)
}

fn enclosing_top_level_form(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let index = enclosing_top_level_index(tree, target)?;
    Some(tree.select_path(&SexprPath::root_child(index)).ok()?.view())
}

/// Walks from `form` down to `target`, accumulating every answer.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it in general, which is exactly what a single depth
/// counter gets wrong.
fn context_within(form: &ExpressionView, target: ByteSpan) -> NodeContext {
    // The starting form's own prefixes count: `'(a (try ...))` is quoted by a
    // prefix on the top-level form itself, which no descent step would see.
    let mut state = QuoteState::EVALUATED.after_prefixes(form);
    let mut parent_head = None;
    let mut view = form;

    loop {
        if view.span == target {
            return NodeContext {
                unevaluated: state.is_data(),
                parent_head,
            };
        }
        let quoting = quoting_form(view);
        // A span naming no node is judged by the innermost node containing it,
        // which is the honest answer for a span a caller synthesized.
        let Some((position, child)) = view
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| span_contains(child.span, target))
        else {
            return NodeContext {
                unevaluated: state.is_data(),
                parent_head,
            };
        };
        state = state.after_prefixes(child);
        // The head of `(quote x)` is `quote` itself, which is not data; only
        // the operands are.
        let is_operand = position != 0;
        state = match quoting {
            Some(QuoteKind::Hard) if is_operand => state.quoted(),
            Some(QuoteKind::Quasi) if is_operand => QuoteState {
                quasi: state.quasi + 1,
                ..state
            },
            Some(QuoteKind::Unquote) if is_operand => QuoteState {
                quasi: state.quasi.saturating_sub(1),
                ..state
            },
            _ => state,
        };
        parent_head = hy_head(view).map(str::to_owned);
        view = child;
    }
}

/// Whether the node at `target` is unevaluated data rather than code.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    node_context(tree, target).unevaluated
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::Hy).expect("parse")
    }

    /// The span of the first list anywhere in the document whose first child is
    /// the atom `head`, in pre-order.
    ///
    /// Deliberately *not* [`hy_head`], which declines a list carrying a reader
    /// prefix. These tests need to locate `'(try …)` and `` `(try …) ``
    /// precisely in order to assert that the rules will decline them.
    fn first_form(tree: &SyntaxTree, head: &str) -> ByteSpan {
        fn walk(view: &ExpressionView, head: &str, found: &mut Option<ByteSpan>) {
            let names_head = view.kind == ExpressionKind::List
                && view.children.first().and_then(hy_atom) == Some(head);
            if found.is_none() && names_head {
                *found = Some(view.span);
            }
            for child in &view.children {
                walk(child, head, found);
            }
        }
        let mut found = None;
        walk(&tree.root_view(), head, &mut found);
        found.expect("a form with that head")
    }

    #[test]
    fn only_a_paren_list_is_a_call() {
        let bracket = tree("[try 1]");
        assert_eq!(hy_head(&bracket.root_view().children[0]), None);

        let dict = tree("{try 1}");
        assert_eq!(hy_head(&dict.root_view().children[0]), None);

        let call = tree("(try 1)");
        assert_eq!(hy_head(&call.root_view().children[0]), Some("try"));
    }

    /// `#(try 1)` is a tuple. It parses as a paren `List`, so the engine's head
    /// index would offer it to a rule registered for `try`.
    #[test]
    fn a_tuple_constant_is_not_a_call() {
        let tuple = tree("#(try 1)");
        let node = &tuple.root_view().children[0];
        assert_eq!(node.delimiter, Some(Delimiter::Paren));
        assert!(!is_call(node));
        assert_eq!(hy_head(node), None);
    }

    /// Python is case sensitive and so is Hy, so `TRY` is an ordinary function
    /// name. The index agrees — it keys non-Common-Lisp heads verbatim — but
    /// this pins the *helper's* behaviour, which is what a rule reads.
    #[test]
    fn a_head_is_compared_exactly() {
        let upper = tree("(TRY 1)");
        assert_eq!(hy_head(&upper.root_view().children[0]), Some("TRY"));
        assert!(!heads_with(&upper.root_view().children[0], "try"));
    }

    /// `,` is an identifier constituent in Hy, not a separator and not an
    /// unquote. `(try, 1)` is a call to a function named `try,`.
    #[test]
    fn a_comma_is_part_of_the_symbol() {
        let comma = tree("(try, 1)");
        assert_eq!(hy_head(&comma.root_view().children[0]), Some("try,"));
        assert!(!heads_with(&comma.root_view().children[0], "try"));
    }

    #[test]
    fn a_bare_form_is_evaluated() {
        let tree = tree("(try (f))");
        assert!(!is_unevaluated_at(&tree, first_form(&tree, "try")));
    }

    #[test]
    fn a_quote_prefix_makes_the_form_data() {
        let tree = tree("'(try (f))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "try")));
    }

    #[test]
    fn a_nested_form_under_a_quote_prefix_is_data() {
        let tree = tree("'(a (try (f)))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "try")));
    }

    #[test]
    fn a_long_hand_quote_form_makes_its_operand_data() {
        let tree = tree("(quote (try (f)))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "try")));
    }

    #[test]
    fn a_form_inside_a_tuple_constant_is_data() {
        let tree = tree("#((try (f)))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "try")));
    }

    /// The documented asymmetry, pinned as a test so that the day `classify_hy`
    /// starts emitting `Unquote` this fails and the header note gets updated
    /// rather than silently becoming a lie.
    ///
    /// `~(f)` is the one spelling that *does* produce a bare `~` atom, because
    /// `(` terminates the identifier — but it still does not clear `quasi`,
    /// because a bare atom is not a reader prefix.
    #[test]
    fn a_hy_unquote_does_not_escape_its_template_yet() {
        let tree = tree("(defmacro m [] `(do ~(try (f))))");
        assert!(
            is_unevaluated_at(&tree, first_form(&tree, "try")),
            "if this now fails, `classify_hy` gained its `~` arms: update the module header"
        );
    }

    /// `~name` is a single atom, so it adds no sibling. This is what makes
    /// arity counting inside a Hy template sound, and it is the fact the Carp
    /// package could not rely on.
    #[test]
    fn a_tilde_symbol_is_one_atom_and_inflates_no_arity() {
        let tree = tree("`(setv ~name 5)");
        let template = &tree.root_view().children[0];
        assert_eq!(template.children.len(), 3);
        assert_eq!(hy_atom(&template.children[1]), Some("~name"));
    }

    #[test]
    fn a_quasiquoted_form_is_data() {
        let tree = tree("(defmacro m [] `(try (f)))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "try")));
    }

    #[test]
    fn the_operator_of_a_quoting_form_is_not_itself_data() {
        let tree = tree("(quote (try (f)))");
        let form = &tree.root_view().children[0];
        let operator = form.children[0].span;
        let operand = form.children[1].span;
        assert!(!node_context(&tree, operator).unevaluated);
        assert!(node_context(&tree, operand).unevaluated);
    }

    #[test]
    fn a_top_level_form_has_no_parent_head() {
        let tree = tree("(try (f))");
        assert_eq!(
            node_context(&tree, first_form(&tree, "try")).parent_head,
            None
        );
    }

    #[test]
    fn a_nested_form_reports_its_immediate_enclosing_head() {
        let tree = tree("(defn f [] (when p (try (g))))");
        assert_eq!(
            node_context(&tree, first_form(&tree, "try"))
                .parent_head
                .as_deref(),
            Some("when")
        );
    }

    /// Without the containment check the binary search returns the *next*
    /// form, and a gap between two top-level forms would inherit its quoting.
    #[test]
    fn a_span_between_two_top_level_forms_is_not_judged_by_its_neighbour() {
        let tree = tree("(a)\n'(b)\n");
        let gap = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(3),
            paredit_core_syntax::sexpr::ByteOffset::new(4),
        );
        assert!(!is_unevaluated_at(&tree, gap));
        assert_eq!(node_context(&tree, gap).parent_head, None);
    }
}
