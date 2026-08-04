//! What this crate's rules share: how to read an atom, how to read a call's
//! head, how much of the surrounding reader syntax says "this is data", and
//! how LFE itself decides what a clause is.
//!
//! Nothing here materializes the document. Both rules in this package work
//! entirely within the subtree the dispatcher handed them — a `when` guard or
//! a single clause list — so neither ever calls [`SyntaxTree::root_view`].
//! That is deliberate: an earlier batch in this workspace measured
//! 450843 ns/call against 28 ns/call purely from a rule reaching `root_view`
//! before its cheap head check, and the cheapest way not to make that mistake
//! is to have no reason to call it at all.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};

/// An atom's text, exactly as the source spells it — *including* any reader
/// prefix the reader did recognize.
///
/// That is what makes this safe to compare against a bare symbol name without
/// also testing `reader_prefixes`: `'when` has `text == "'when"`, which is not
/// equal to `"when"`, so quoted data can never be read as an operator.
#[must_use]
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The exact head symbol of a `(...)` list.
///
/// Paren-delimited only. `head_key` returns the head verbatim for LFE — there
/// is no case folding — so callers compare byte for byte.
#[must_use]
pub(crate) fn head_symbol(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren))
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

/// Whether `view` is a `(...)` list at all.
#[must_use]
pub(crate) fn is_paren_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren)
}

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// `,x` inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a `,x` inside `` `(…) `` is meant to escape back to code, so
/// `quasi` counts up and down. A single `i32` depth counter cannot express
/// that and has shipped elsewhere in this workspace as a false-positive source
/// twice.
///
/// Unlike Carp, LFE genuinely uses `,` and `,@` as unquote and
/// unquote-splicing, so the [`ReaderPrefix::Unquote`] arm is live here rather
/// than defensive: LFE macros are written as `` `(…) `` templates with `,`
/// escapes, and a rule that treated the escaped part as data would miss real
/// findings in generated code.
///
/// This is a copy of the model in
/// `packages/feature/lint-condition-system/src/support.rs`, as the other
/// dialect packages also copy it. It should move to a shared home; see this
/// package's README.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    pub(crate) const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    pub(crate) const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// Whether the subtree can be pruned outright rather than merely not
    /// reported on.
    ///
    /// The distinction matters and getting it wrong silently loses findings.
    /// Inside a hard `'` nothing can escape back to code — a `,` there is a
    /// comma character in a literal list — so the whole subtree is data and
    /// walking it is wasted work. Inside a `` ` `` a `,` *does* escape back,
    /// so a quasiquoted node must still be descended into even though the node
    /// itself is data.
    ///
    /// Collapsing the two (pruning on `is_data`) makes every unquoted call
    /// inside a macro template invisible, which is where LFE puts most of its
    /// generated code.
    pub(crate) const fn is_pruned(self) -> bool {
        self.hard
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// LFE's `#B(…)`, `#M(…)` and `#S(…)` are deliberately neutral: none of
    /// them turns code into data. They are literal *constructors* whose
    /// elements are still evaluated.
    #[must_use]
    pub(crate) fn after_prefixes(mut self, view: &ExpressionView) -> Self {
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

    /// The long-hand `(quote …)` / `(quasiquote …)`, which LFE macros spell
    /// out and which `lfe_macro` treats identically to the reader shorthand.
    #[must_use]
    pub(crate) fn after_quoting_head(mut self, view: &ExpressionView) -> Self {
        match head_symbol(view) {
            Some("quote") => self.hard = true,
            Some("quasiquote" | "backquote") => self.quasi += 1,
            Some("unquote" | "unquote-splicing") => {
                self.quasi = self.quasi.saturating_sub(1);
            }
            _ => {}
        }
        self
    }
}

/// Whether an atom in pattern position is a *variable* rather than a literal.
///
/// In LFE a bare symbol in a pattern is a variable and therefore matches
/// anything; an atom literal has to be quoted (`'one`), which the reader gives
/// a [`ReaderPrefix::Quote`] and a `text` that still carries the `'`. So the
/// test is: an unprefixed atom whose spelling starts like a symbol.
///
/// Deliberately conservative. Requiring the first byte to be alphabetic or `_`
/// rejects numbers, strings, characters and `#B(…)`-style literals without
/// having to enumerate them, and anything it wrongly rejects merely costs a
/// finding rather than inventing one.
///
/// # The prefix test kills nothing, and stays anyway
///
/// Mutation-testing found the `reader_prefixes` test redundant: removing it
/// failed no test, and no test could be written for it. For LFE this reader
/// puts every atom-level prefix into `text` as well — measured, `'lit` is
/// `text: "'lit"` with `[Quote]`, `,v` is `",v"` with `[Unquote]`, `` `t `` is
/// ``"`t"`` with `[Quasiquote]`, `,@s` is `",@s"` with `[UnquoteSplicing]` —
/// so the first-byte test already rejects all four, none of `'`, `,` or
/// `` ` `` being alphabetic.
///
/// It stays because the two tests mean different things. The first-byte test
/// asks "does this spell a symbol"; this one asks "did the reader mark it as
/// something other than a bare symbol". They agree today only because of a
/// choice `reader_policy` made for LFE and *not* for Carp, where the same
/// reader hands back `@x` as the atom `x` carrying a `Copy` prefix with the
/// sigil stripped from the text. A helper that leaned on the LFE spelling
/// would be correct by accident.
#[must_use]
pub(crate) fn is_variable_atom(view: &ExpressionView) -> bool {
    if !view.reader_prefixes.is_empty() {
        return false;
    }
    let Some(text) = atom_text(view) else {
        return false;
    };
    text.as_bytes()
        .first()
        .copied()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
}

/// LFE's own test for whether `(defun name ARGS . body)` is the traditional
/// single-clause form rather than a set of match clauses.
///
/// This is not a heuristic of this package's invention. `lfe_macro.erl:1257`
/// calls the choice an "educated guess" and makes it with
/// `lfe_lib:is_symb_list(Args)`:
///
/// ```text
/// exp_defun([Args|Body]=Rest) ->
///     case lfe_lib:is_symb_list(Args) of
///         true  -> exp_lambda_defun(Args, Body);
///         false -> exp_match_defun(Rest)
///     end.
/// ```
///
/// and `lfe_lib.erl:39` defines it as "every element is an atom":
///
/// ```text
/// is_symb_list([S|Ss]) when is_atom(S) -> is_symb_list(Ss);
/// is_symb_list([]) -> true;
/// is_symb_list(_) -> false.
/// ```
///
/// The empty list is a symbol list, so `(defun f () body)` is traditional.
///
/// The one place this has to be more careful than the Erlang code is quoting.
/// `'one` is *not* an atom to LFE — it reads as the two-element list
/// `(quote one)` — so `(defun c (('one) 1) …)` is a match-defun. The reader in
/// this workspace gives `'one` back as an `Atom` carrying a `Quote` prefix, so
/// testing `kind == Atom` alone would call it a symbol list and get the
/// opposite answer. [`is_plain_symbol`] requires the prefix list to be empty
/// for exactly this reason.
#[must_use]
pub(crate) fn is_symb_list(view: &ExpressionView) -> bool {
    is_paren_list(view) && view.children.iter().all(is_plain_symbol)
}

/// An atom carrying no reader prefix — what LFE's `is_atom/1` would accept.
#[must_use]
fn is_plain_symbol(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty()
}

/// The heads whose bodies are *syntax-rules templates* rather than code.
///
/// LFE's Scheme-compatibility layer provides pattern/template macros in which
/// the template is written literally, with no quote and no backquote —
/// `scm.erl:47-59` rewrites all of them into `define-macro`:
///
/// ```text
/// ['define-syntax',Name,Def]  -> exp_syntax(Name, Def)
/// ['let-syntax',Defs|Body]    -> ...
/// [defsyntax,Name|Rules]      -> exp_rules(Name, [], Rules)
/// ```
///
/// and `exp_syntax/2` accepts either `[macro|Cls]` or `['syntax-rules'|Rules]`.
///
/// This matters because the quote model cannot see these. A template inside
/// `` ` `` is data; a template inside `defsyntax` looks exactly like code. The
/// corpus audit found this the hard way: LFE's own `dev/test_macro.lfe:27`
/// contains
///
/// ```text
/// (defsyntax c-ond
///   ([('else . b)] (begin . b))
///   ([(('?= p e) . b) . c] (case e (p . b) (_ (c-ond . c))))
///   ...)
/// ```
///
/// where `p` is a *pattern variable* that will be replaced by whatever pattern
/// the caller wrote. Read as code, `(p . b)` is a bare variable and therefore a
/// catch-all, which makes the `(_ …)` after it look dead. It is not: `p`
/// becomes a real pattern at expansion. That was a genuine false positive on
/// LFE's own source, and suppressing these forms is what removes it.
///
/// The `scm:` prefix is stripped before comparing, because the guide spells
/// them `scm:defsyntax` and `scm:define-syntax` as often as bare.
const TEMPLATE_HEADS: [&str; 5] = [
    "defsyntax",
    "define-syntax",
    "let-syntax",
    "syntaxlet",
    "syntax-rules",
];

/// Whether `head` introduces a syntax-rules template.
fn is_template_head(head: &str) -> bool {
    let bare = head.strip_prefix("scm:").unwrap_or(head);
    TEMPLATE_HEADS.contains(&bare)
}

/// What one descent from the root can say about a node's context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeContext {
    /// Inside a quote or an unescaped quasiquote.
    pub(crate) is_data: bool,
    /// Inside a `defsyntax`-family template, where the "code" is a pattern
    /// template and its symbols are pattern variables.
    pub(crate) in_syntax_template: bool,
}

impl NodeContext {
    /// Whether a finding at this node should be withheld.
    pub(crate) const fn suppresses_findings(self) -> bool {
        self.is_data || self.in_syntax_template
    }
}

/// Everything one materialization of the document can answer about `target`.
///
/// # Call this last, never first
///
/// [`SyntaxTree::root_view`] materializes the whole document into owned views,
/// so this costs the *file's size* rather than the node's depth. A rule that
/// asked this before its cheap head check would charge every visited node for
/// a walk that almost always answers "no"; an earlier batch in this workspace
/// measured 450843 ns/call against 28 ns/call from exactly that ordering
/// mistake. Both callers here ask only once a finding is otherwise ready to
/// report, which makes the cost proportional to the number of findings rather
/// than the number of nodes — and findings are rare.
///
/// Both questions are answered from the *same* descent for the same reason:
/// asking them separately would materialize the document twice.
///
/// # What it answers
///
/// `is_data` is read *at* the target, not at any ancestor. A hard `'` never
/// clears, so anything textually inside one is data. A `` ` `` clears at a `,`.
///
/// `in_syntax_template` is true if any ancestor is a [`TEMPLATE_HEADS`] form,
/// because a template does not stop being a template partway down.
#[must_use]
pub(crate) fn node_context(tree: &SyntaxTree, target: ByteSpan) -> NodeContext {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;
    let mut in_syntax_template = false;
    loop {
        if head_symbol(view).is_some_and(is_template_head) {
            in_syntax_template = true;
        }
        let quoting = view;
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return NodeContext {
                is_data: state.is_data(),
                in_syntax_template,
            };
        };
        state = state.after_quoting_head(quoting).after_prefixes(child);
        view = child;
        if view.span == target {
            // The target itself may be the template form.
            if head_symbol(view).is_some_and(is_template_head) {
                in_syntax_template = true;
            }
            return NodeContext {
                is_data: state.is_data(),
                in_syntax_template,
            };
        }
    }
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The guard attached to a clause, if it has one.
///
/// A clause is `(Pattern . Body)` and an optional guard is the *first* body
/// element spelled `(when …)` — `lfe_macro.erl:599` matches exactly
/// `exp_clause([P,['when'|G]|B], …)`. Anything else in that position is
/// ordinary body code.
#[must_use]
pub(crate) fn clause_guard(
    clause: &ExpressionView,
    pattern_index: usize,
) -> Option<&ExpressionView> {
    clause
        .children
        .get(pattern_index + 1)
        .filter(|candidate| head_symbol(candidate) == Some("when"))
}
