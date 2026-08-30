//! What this crate's rules share: how to read an atom, how to read a call's
//! head, and how much of the surrounding reader syntax says "this is data".
//!
//! Nothing here runs per visited node. Every helper is called from inside a
//! rule that has already matched its head, and the expensive one
//! (`is_unevaluated_at`) is called only once a finding is otherwise ready to
//! report — it descends from [`SyntaxTree::root_view`], which materializes the
//! whole document, so calling it before the cheap head check costs orders of
//! magnitude more per invocation than the check it guards.
//!
//! # What this workspace's reader does with Carp
//!
//! Carp's own guide (`docs/LanguageGuide.md`, "Reader Macros") defines two:
//!
//! ```text
//! &x ;; same as (ref x)
//! @x ;; same as (copy x)
//! ```
//!
//! This workspace's reader now implements both, along with `~x` (deref) and
//! `$[…]` (static array). `reader_policy.rs` gives Carp its own
//! `classify_carp` rather than routing it through the permissive
//! `classify_legacy`, which knew `#;`, `#_`, `#+`, `#-` and the shared quote
//! prefixes and nothing about `@` or `&`.
//!
//! That history still constrains the rules here, because the rules were built
//! against the old reading and stay correct under both:
//!
//! - `@x` and `&x` used to lex as *one atom* whose text included the sigil
//!   (`"@x"`); they are now the atom `x` carrying a [`ReaderPrefix::Copy`] or
//!   [`ReaderPrefix::Ref`]. A head or symbol comparison must therefore not
//!   assume either shape — `atom_text` compares the text the reader produced.
//! - `@(f x)` and `&(f x)` used to lex as a **bare `@` atom followed by a
//!   sibling list**, inflating the enclosing call's arity by one at 1493 sites
//!   in 116 of the upstream corpus's 248 files. They are now a single prefixed
//!   form, so arity is trustworthy again — but every rule here still keys on
//!   the head symbol alone, which was never weaker than counting arguments.
//!
//! Carp's unquote is `%` and its unquote-splicing is `%@`
//! (`docs/Quasiquotation.md`), neither of which the reader recognizes as a
//! prefix; that is a deliberate omission rather than an oversight, since
//! recognizing them would make the interior of every `` ` `` template read as
//! code and *add* findings on macro bodies. `QuoteState::quasi` therefore
//! never counts back down for Carp, so everything textually inside a `` ` ``
//! template reads as data. That suppresses findings rather than inventing
//! them, which is the side to be wrong on.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};

/// An atom's text, exactly as the source spells it — *including* any reader
/// prefix the reader did recognize.
///
/// That is what makes this safe to compare against a bare symbol name without
/// also testing `reader_prefixes`: `'=>` has `text == "'=>"`, which is not
/// equal to `"=>"`, so quoted data can never be read as an operator.
/// The kind test is, today, redundant with the reader: `tree.rs:1022`
/// populates `text` with `(node.kind == NodeKind::Atom)`, so a list's `text` is
/// always `None` and no input can tell the two spellings apart.
/// Mutation-testing confirmed it — removing the kind test killed no test, and
/// unlike the paren-only test in [`head_symbol`] no test *could* be written
/// for it.
///
/// It stays for the reason the head index states about itself: a helper that
/// leaned on another component's current shape for its notion of what an atom
/// is would be correct only by accident.
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The exact head symbol of a `(...)` list.
///
/// Paren-delimited only. In Carp a `[...]` is an array literal or a parameter
/// vector and a `{...}` is a map literal — none of them is a call — so reading
/// a "head" off one would invent an operator the reader never produced.
pub(crate) fn head_symbol(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren))
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// `%` inside `'(…)` is a percent character in a literal list, so `hard` never
/// clears; a `%` inside `` `(…) `` is meant to escape back to code, so `quasi`
/// would count up and down. A single `i32` depth counter cannot express that
/// and has shipped elsewhere in this workspace as a false-positive source.
///
/// As the module header records, the reader gives Carp no `Unquote` prefix at
/// all, so in practice `quasi` only ever counts up here. The
/// [`ReaderPrefix::Unquote`] arm is kept because it is what makes this a
/// correct quote model rather than one that happens to work on the inputs the
/// reader can currently produce.
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
    /// code into data.
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
/// which hand-written Carp macros spell out — and `(quasiquote …)`, which
/// `docs/Quasiquotation.md` presents as the primary spelling with `` ` `` as
/// the literal shorthand.
fn quoting_head(view: &ExpressionView) -> Option<&str> {
    head_symbol(view).filter(|head| matches!(*head, "quote" | "quasiquote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The heads that bind a name in Carp.
///
/// `defmacro`, `defndynamic` and `defdynamic` bind at macro-expansion time;
/// `defn` and `def` bind at runtime. Any of them shadowing a core threading
/// macro is enough to make a rename stop being meaning-preserving.
const DEFINING_HEADS: [&str; 5] = ["defmacro", "defndynamic", "defdynamic", "defn", "def"];

/// Everything one materialization of the document can answer at once.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeContext {
    /// Whether the node is unevaluated data rather than code.
    pub(crate) is_data: bool,
    /// Whether this file defines the name that was asked about.
    pub(crate) defines_asked_name: bool,
}

/// Both facts about `target`, from a single materialization of the document.
///
/// This exists for cost, not tidiness. [`SyntaxTree::root_view`] materializes
/// the whole document into owned views, so its cost is the file's size, not
/// the node's depth; a rule that asked the two questions separately paid that
/// twice. Measured on a 67140-byte dense Carp fixture, splitting them cost
/// 8589447 ns/call — four orders of magnitude above the shipped comparator in
/// the same pass — and merging them is the single largest saving available
/// without changing the core API.
///
/// This is why the caller applies its cheap head check first and reaches this
/// only with a finding otherwise ready to report.
///
/// # The two answers
///
/// **`is_data`** is read *at* the target and nowhere shallower. An ancestor
/// being quasiquoted does not settle it; being inside a hard `'` does, and
/// that is already modelled by `hard` never clearing. A span that names no
/// node is judged by the innermost node containing it, which is the honest
/// answer for a span a caller synthesized rather than took from the tree.
///
/// **`defines_asked_name`** scans *definition sites only*: top-level forms and
/// the bodies of `defmodule`/`with`/`do` forms, which is where Carp puts
/// `defn` and friends. It deliberately does not descend into function bodies,
/// because Carp has no internal defines — a `defn` cannot appear inside
/// another `defn`'s body — so a full walk would pay for the whole document to
/// find nothing. It is otherwise blind to module nesting, answering "does this
/// file define this name anywhere?", which over-approximates shadowing.
/// Over-approximating *withholds a fix*, the safe direction; under-approximating
/// would rewrite code whose meaning it had not established.
#[must_use]
pub(crate) fn node_context(tree: &SyntaxTree, target: ByteSpan, name: &str) -> NodeContext {
    let root = tree.root_view();

    // -- the descent, for the quote verdict --------------------------------
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;
    let is_data = loop {
        let quoting = quoting_head(view).is_some();
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            break state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            break state.is_data();
        }
    };

    // -- the definition-site scan, for the shadowing verdict ---------------
    const CONTAINER_HEADS: [&str; 3] = ["defmodule", "with", "do"];
    let mut defines_asked_name = false;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        let Some(head) = head_symbol(view) else {
            continue;
        };
        if DEFINING_HEADS.contains(&head) {
            let defined = view.children.get(1).and_then(atom_text);
            if defined == Some(name) {
                defines_asked_name = true;
                break;
            }
            continue;
        }
        if CONTAINER_HEADS.contains(&head) {
            stack.extend(view.children.iter());
        }
    }

    NodeContext {
        is_data,
        defines_asked_name,
    }
}
