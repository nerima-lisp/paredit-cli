//! Shared primitives for the Fennel and Janet rules.
//!
//! Three of the shared one-liners in [`paredit_core_syntax::view_query`] are
//! *wrong* for these two dialects and are deliberately not used here:
//!
//! - `symbol_is`/`symbol_in`/`unqualified` case-fold and strip a package
//!   qualifier, which is Common Lisp reader behaviour. Fennel and Janet are
//!   both case-sensitive, and `:` is not a package marker in either: in Fennel
//!   it introduces a method multi-sym (`handle:read`) and a string literal
//!   (`:keyword`), and in Janet it introduces a keyword. `unqualified` turns
//!   `handle:read` into `read`, so `symbol_is("handle:read", "read")` is true —
//!   a rule keyed on a core operator would match an unrelated method call.
//!   Everything here compares with `==`.
//! - `atom_text` returns the atom's *whole* text, reader prefixes included, so
//!   the `,x` of a macro template reads as `",x"`. [`symbol_text`] strips them.
//!
//! The quote model is the two-counter one from
//! `paredit_feature_lint_condition_system::support`, which is private to that
//! crate; a single `i32` depth counter is wrong (`'` never clears, `` ` ``
//! does) and has shipped as a false-positive source before. Both dialects have
//! `'`/`` ` ``/`,` — Fennel spells quasiquote `` ` `` and Janet spells it `~`,
//! and the reader maps both onto [`ReaderPrefix::Quasiquote`] — so the guard is
//! needed here for exactly the same reason it is needed in Common Lisp.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};

/// The atom's own symbol text, with any reader prefix removed.
///
/// `text` spans the prefixes too, so `,x` in a Fennel macro body reads as
/// `",x"` without this and would never compare equal to `x`.
#[must_use]
pub fn symbol_text(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    view.text.as_deref()?.get(view.symbol_offset..)
}

/// The head symbol of a `(...)` list, exactly as written.
///
/// Bracket and brace forms have no head: they are data literals in both
/// dialects, and the engine's head index only consults `list_head` for paren
/// lists anyway (`paredit_core_lint_engine::engine::dispatch`), so a rule
/// using [`HeadFilter::Heads`] can never be handed one.
///
/// [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads
#[must_use]
pub fn head_symbol(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }
    view.children.first().and_then(symbol_text)
}

/// Whether `view` is a `(...)` call to exactly one of `heads`, compared
/// byte-for-byte.
#[must_use]
pub fn calls_exactly(view: &ExpressionView, heads: &[&str]) -> bool {
    head_symbol(view).is_some_and(|head| heads.contains(&head))
}

/// Whether any atom anywhere under `view` spells exactly `name`.
///
/// Deliberately blind to quoting and to shadowing: every caller here uses it
/// to *suppress* a finding, so over-matching costs a false negative and
/// under-matching would cost a false positive.
#[must_use]
pub fn mentions_symbol(view: &ExpressionView, name: &str) -> bool {
    let mut stack = vec![view];
    while let Some(node) = stack.pop() {
        if symbol_text(node) == Some(name) {
            return true;
        }
        stack.extend(node.children.iter());
    }
    false
}

/// Whether `view` is a literal whose value is immutable in Janet.
///
/// Janet's mutable containers all carry the `@` prefix, which the reader
/// records as [`ReaderPrefix::HashLiteral`]: `@[…]` is an array and `[…]` a
/// tuple, `@{…}` a table and `{…}` a struct, `@"…"` a buffer and `"…"` a
/// string. The absence of that one prefix is the whole distinction.
#[must_use]
pub fn is_immutable_janet_literal(view: &ExpressionView) -> bool {
    if view.reader_prefixes.contains(&ReaderPrefix::HashLiteral) {
        return false;
    }
    match view.kind {
        ExpressionKind::List => {
            matches!(view.delimiter, Some(Delimiter::Bracket | Delimiter::Brace))
        }
        ExpressionKind::Atom => symbol_text(view).is_some_and(|text| text.starts_with('"')),
        ExpressionKind::Root => false,
    }
}

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down. A single depth counter cannot express that.
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
    /// `HashLiteral` is deliberately neutral: Janet's `@` marks mutability,
    /// not quoting, and `@[(f x)]` evaluates `(f x)`.
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

/// The long-hand `(quote …)`. Fennel's reader expands `'x` and `` `x `` to it,
/// and a macro body written by hand spells it out.
fn is_quote_form(view: &ExpressionView) -> bool {
    head_symbol(view) == Some("quote")
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// # Cost
///
/// [`SyntaxTree::root_view`] materializes the whole document — a `Vec` per node
/// and a `String` per atom — so this is O(file), not O(depth), despite the
/// descent below being O(depth). **Call it only once a finding exists.** A
/// measured pass over a 63 KB Fennel file charged 450 µs per invocation to a
/// rule that asked on every head-matched node, against 39 ns for a shipped
/// `AllNodes` rule over the same tree; asking after the domain check instead
/// removed the cost entirely, because correct code produces no findings to ask
/// about. Use [`is_unevaluated_in`] when a root view is already in hand.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    is_unevaluated_in(&tree.root_view(), target)
}

/// Whether *either* the form the engine dispatched on or the node a rule wants
/// to report is unevaluated data.
///
/// Both questions are needed, and neither implies the other. A third-party
/// sweep over 340 Fennel files produced a false positive for each direction:
///
/// - `` `(and ,condition ,(unpack guards)) `` in `fennel/src/fennel/match.fnl`.
///   The reported node is the `,(unpack guards)`, whose `,` escapes the
///   quasiquote, so *it* is code — but the `(and …)` around it is a template
///   and no truncation will ever happen. Only the **form** span rejects this.
/// - `` (macro m [expr] `(do ,expr ,expr)) `` in `fennel/test/loops.fnl` and
///   nine more. The dispatched `(macro …)` form is ordinary code — but the
///   `(do …)` being reported is a list the macro *constructs*, not a `do` that
///   runs, and its `do` is what lets the expansion return two forms at once.
///   Only the **reported** span rejects this.
///
/// Suppressing on either is deliberately the conservative direction: it can
/// only cost a true positive, never invent one.
///
/// # Cost
///
/// One [`SyntaxTree::root_view`] for both questions rather than one each, which
/// matters because that call is O(file). Still to be called **only once a
/// finding exists** — see [`is_unevaluated_at`].
#[must_use]
pub fn is_unevaluated_either(tree: &SyntaxTree, form: ByteSpan, reported: ByteSpan) -> bool {
    let root = tree.root_view();
    is_unevaluated_in(&root, form) || is_unevaluated_in(&root, reported)
}

/// [`is_unevaluated_at`] against a root view the caller already has.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost really is the node's depth.
///
/// The verdict is read *at* the target and nowhere shallower: `` `(do ,(f)) ``
/// has a quasiquoted ancestor and an evaluated target. Being inside a hard `'`
/// does settle it, and that is already modelled by `hard` never clearing.
#[must_use]
pub fn is_unevaluated_in(root: &ExpressionView, target: ByteSpan) -> bool {
    let mut view: &ExpressionView = root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state.is_data();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn forms(source: &str, dialect: Dialect) -> Vec<ExpressionView> {
        SyntaxTree::parse_with_dialect(source, dialect)
            .expect("parse")
            .root_view()
            .children
            .clone()
    }

    #[test]
    fn symbol_text_strips_a_reader_prefix() {
        let form = forms("`(do ,x)", Dialect::Fennel).remove(0);
        let unquoted = &form.children[1];
        assert_eq!(unquoted.text.as_deref(), Some(",x"));
        assert_eq!(symbol_text(unquoted), Some("x"));
    }

    #[test]
    fn head_comparison_is_case_sensitive_and_keeps_the_colon() {
        // `symbol_is` from view_query would answer true to both of these.
        let method = forms("(handle:read)", Dialect::Fennel).remove(0);
        assert_eq!(head_symbol(&method), Some("handle:read"));
        assert!(!calls_exactly(&method, &["read"]));

        let upper = forms("(Var x 1)", Dialect::Fennel).remove(0);
        assert!(!calls_exactly(&upper, &["var"]));
    }

    #[test]
    fn a_bracket_form_has_no_head() {
        let bracket = forms("[var x 1]", Dialect::Fennel).remove(0);
        assert_eq!(head_symbol(&bracket), None);
    }

    #[test]
    fn janet_mutability_is_read_off_the_at_prefix() {
        let form = forms(
            "(f [1] @[1] {:a 1} @{:a 1} \"s\" @\"s\" xs)",
            Dialect::Janet,
        )
        .remove(0);
        let verdicts: Vec<bool> = form.children[1..]
            .iter()
            .map(is_immutable_janet_literal)
            .collect();
        assert_eq!(
            verdicts,
            vec![true, false, true, false, true, false, false],
            "tuple/struct/string are immutable; array/table/buffer and a symbol are not"
        );
    }

    #[test]
    fn a_hard_quote_never_clears_but_a_quasiquote_does() {
        let tree = SyntaxTree::parse_with_dialect("'(a ,(b))", Dialect::Fennel).expect("parse");
        let inner = tree.root_view().children[0].children[1].span;
        assert!(
            is_unevaluated_at(&tree, inner),
            "a comma inside a hard quote is a comma character"
        );

        let tree = SyntaxTree::parse_with_dialect("`(a ,(b))", Dialect::Fennel).expect("parse");
        let inner = tree.root_view().children[0].children[1].span;
        assert!(
            !is_unevaluated_at(&tree, inner),
            "a comma inside a quasiquote escapes back to code"
        );
    }

    #[test]
    fn janets_tilde_quasiquote_is_the_same_state() {
        let tree = SyntaxTree::parse_with_dialect("~(a ,(b))", Dialect::Janet).expect("parse");
        let quoted = tree.root_view().children[0].children[0].span;
        let unquoted = tree.root_view().children[0].children[1].span;
        assert!(is_unevaluated_at(&tree, quoted));
        assert!(!is_unevaluated_at(&tree, unquoted));
    }

    #[test]
    fn mentions_symbol_walks_the_whole_subtree() {
        let form = forms("(fn [] (let [y 1] (set acc y)))", Dialect::Fennel).remove(0);
        assert!(mentions_symbol(&form, "acc"));
        assert!(!mentions_symbol(&form, "ACC"));
        assert!(!mentions_symbol(&form, "ac"));
    }
}
