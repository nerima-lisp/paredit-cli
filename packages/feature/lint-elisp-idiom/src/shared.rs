//! What the rules in this crate share: how to read an atom, and how to tell
//! code from data.
//!
//! Nothing here runs per visited node. Every helper is called from inside a
//! rule that has already matched its head, and the two expensive ones
//! ([`is_unevaluated_at`] and [`find_definition`]) are called only once a
//! finding is otherwise ready to report. The `clean/forms/*` benchmarks lint
//! files with zero findings, so the per-file cost of a rule that matches
//! nothing is exactly what they measure.

use paredit_core_syntax::emacs_lisp::{EmacsLispCallableShape, EmacsLispOperator};
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionPath, ExpressionView, ReaderPrefix, SyntaxTree,
};

/// An atom's text, exactly as the source spells it — *including* any reader
/// prefix.
///
/// That is what makes this safe to compare against a bare symbol name without
/// also testing `reader_prefixes`: `#'defun` has `text == "#'defun"`, which is
/// not equal to `"defun"`, so a function designator can never be read as a
/// definition head. An explicit `reader_prefixes.is_empty()` conjunct was
/// written here first and mutation-testing showed it killed nothing, because
/// every caller compares for equality against an unprefixed name. Use
/// [`prefixed_atom_text`] when the prefix is the thing being read past.
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// An atom's *symbol* text, past any reader prefix.
///
/// `view.text` for `#'my-go` is the whole four-and-more characters including
/// the `#'`; `symbol_offset` is where the symbol itself starts. Comparing the
/// unsliced text against `"my-go"` silently never matches, which is a rule
/// that reports nothing rather than one that fails to compile.
pub(crate) fn prefixed_atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
        .and_then(|text| text.get(view.symbol_offset..))
}

/// The exact head symbol of a `(...)` list.
pub(crate) fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List)
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

/// The operator a list's head names, matched the way Emacs Lisp reads it.
///
/// A note on why every rule here re-reads its head, since it is easy to state
/// this wrongly. `head_index::head_key` case-folds **only** for
/// `Dialect::CommonLisp`; an Emacs Lisp head is indexed exactly as written, so
/// `(Define-Key …)` never reaches a rule whose `Heads` say `define-key` in the
/// first place. Mutation-testing confirmed it: turning every one of these
/// comparisons case-insensitive killed no test.
///
/// They stay anyway, because the head index documents itself as a *pre-filter*
/// that "must never be narrower than any rule's notion of the same operator,
/// but may be wider". A rule that leaned on the index for case would be
/// correct only by accident of the dialect it happens to run on.
pub(crate) fn emacs_lisp_operator(view: &ExpressionView) -> Option<EmacsLispOperator> {
    EmacsLispOperator::from_head(list_head(view)?)
}

/// The text of an Emacs Lisp string literal, without its quotes, or `None` for
/// anything that is not one.
///
/// Escape sequences are left exactly as written. Every caller here counts
/// newline *characters*, and a `\n` inside a `"…"` in the source is two bytes
/// that Emacs reads as one newline — so [`interactive_argument_count`] looks
/// for the escape, not the byte.
pub(crate) fn string_literal_body(view: &ExpressionView) -> Option<&str> {
    let text = atom_text(view)?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    // `"\""` strips to `\`, which is a one-character body only because the
    // closing quote was escaped. Refusing it costs one contrived spec and
    // keeps every caller's character arithmetic honest.
    (!inner.ends_with('\\')).then_some(inner)
}

/// `'foo`, `#'foo` or `(quote foo)`/`(function foo)` read as `foo`.
///
/// `None` for a bare symbol: a keymap binding written `(define-key m k foo)`
/// passes the *value* of `foo`, which this crate cannot read, and guessing
/// that the author meant the symbol would report code that is merely unusual.
pub(crate) fn designated_symbol(view: &ExpressionView) -> Option<&str> {
    if view.kind == ExpressionKind::List {
        return match list_head(view) {
            Some("quote" | "function") => view.children.get(1).and_then(atom_text),
            _ => None,
        };
    }
    let designating = view
        .reader_prefixes
        .iter()
        .any(|prefix| matches!(prefix, ReaderPrefix::Quote | ReaderPrefix::Function));
    designating.then(|| prefixed_atom_text(view)).flatten()
}

/// Whether `view` is a `lambda` form, in any of the three spellings that
/// evaluate to a function.
///
/// A *quote-prefixed* lambda answers `false`: `'(lambda …)` is a list, not a
/// function, and `elisp-quoted-lambda` already says so with a strictly
/// stronger complaint than any rule here would add.
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
    /// `#'`, `#+`, metadata and the rest are deliberately neutral: none of
    /// them turns code into data.
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
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor
/// being data does not settle it: `` `(a ,(add-hook 'h (lambda ()))) `` has a
/// quasiquoted ancestor and an evaluated target. Being inside a hard `'` does
/// settle it, and that is already modelled by `hard` never clearing.
#[must_use]
pub(crate) fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
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

// ---------------------------------------------------------------------------
// Command definitions
// ---------------------------------------------------------------------------

/// A function definition form and the shape its operator implies.
pub(crate) struct Definition<'a> {
    pub(crate) view: &'a ExpressionView,
    pub(crate) shape: EmacsLispCallableShape,
}

/// The `defun`-family form in `tree` that defines `name`, if the file has one,
/// as an owned subtree paired with the shape its operator implies.
///
/// Only *top-level* forms are searched. A `defun` nested inside a `let` or a
/// `progn` is still a definition at load time, but a name defined in one of
/// those is as likely to be conditional as not, and reporting on a definition
/// this crate cannot see the guard for would be a guess.
///
/// Returns `None` for a name this file does not define at all, which is the
/// only honest answer for a command that lives in another file.
///
/// **Deliberately does not build the root view.** `SyntaxTree::root_view()`
/// materializes the whole document — a `Vec` per node and a `String` per atom
/// — and this runs once per key binding, so calling it here made the rule cost
/// O(bindings × file). Measured before and after on a file of N `defun`s and N
/// `define-key`s: the root-view version ran 17.9µs / 35.2µs / 75.6µs per
/// binding at N = 32 / 64 / 128, a 4x cost for a 2x file. What replaces it is
/// two cheap passes per candidate — a borrowed `head()` and a borrowed source
/// slice — and a subtree materialization only for the form whose own text
/// contains the name at all.
///
/// The `contains` is a *pre-filter*, never the verdict: a `defun` whose
/// docstring happens to mention the name passes it and is then rejected by the
/// exact comparison below, and a form that does not contain the name cannot
/// possibly define it.
pub(crate) fn find_definition(
    tree: &SyntaxTree,
    name: &str,
) -> Option<(ExpressionView, EmacsLispCallableShape)> {
    for index in 0..tree.root_children().len() {
        let Ok(selection) = tree.select_path(&ExpressionPath::root_child(index)) else {
            continue;
        };
        let Some(head) = selection.head() else {
            continue;
        };
        let Some(shape) =
            EmacsLispOperator::from_head(head).and_then(EmacsLispOperator::callable_shape)
        else {
            continue;
        };
        // A macro is expanded, never called, so it cannot be a command at all
        // — `elisp-interactive-in-macro` owns that complaint.
        if !shape.accepts_interactive() {
            continue;
        }
        if !selection.text().contains(name) {
            continue;
        }
        let view = selection.view();
        if view.children.get(1).and_then(atom_text) == Some(name) {
            return Some((view, shape));
        }
    }
    None
}

impl Definition<'_> {
    /// The `(interactive …)` header of this definition, if it has one.
    ///
    /// The header runs from the body start past the docstring and any
    /// `(declare …)`; the first form that is none of those ends it. An
    /// `(interactive …)` deeper in the body is an ordinary call that returns
    /// nil, not a command specification, and is not what this looks for.
    pub(crate) fn interactive_header(&self) -> Option<&ExpressionView> {
        let mut index = self.shape.body_start_child_index();
        while let Some(child) = self.view.children.get(index) {
            if child.kind == ExpressionKind::Atom {
                // A docstring, which precedes the header.
                index += 1;
                continue;
            }
            match list_head(child) {
                Some("declare" | "cl-declare") => index += 1,
                Some("interactive") => return Some(child),
                _ => return None,
            }
        }
        None
    }

    /// The definition's own name, for the message.
    pub(crate) fn name(&self) -> &str {
        self.view.children.get(1).and_then(atom_text).unwrap_or("?")
    }

    /// How many arguments this definition *requires*, counting up to the first
    /// lambda-list keyword.
    ///
    /// Everything from `&optional` on is optional or collected, so a command
    /// whose interactive spec supplies nothing still calls cleanly. `cl-defun`
    /// adds `&key`, `&aux` and `&rest`; stopping at the first `&`-prefixed
    /// token covers all of them without this crate enumerating a vocabulary
    /// that belongs to `core/syntax`.
    pub(crate) fn required_argument_count(&self) -> usize {
        let Some(arglist) = self.view.children.get(self.shape.arglist_child_index()) else {
            return 0;
        };
        if arglist.kind != ExpressionKind::List {
            return 0;
        }
        arglist
            .children
            .iter()
            .take_while(|child| atom_text(child).is_none_or(|text| !text.starts_with('&')))
            .count()
    }
}

/// How many arguments an `(interactive …)` header supplies, or `None` when
/// that cannot be read from the source.
///
/// Three cases, and the third is why this returns an `Option`:
///
/// - **No descriptor.** `(interactive)` passes no arguments. Verified against
///   GNU Emacs 31.0.91: `(defun f (a) (interactive) …)` is `commandp`, and
///   `call-interactively` on it signals `wrong-number-of-arguments`.
/// - **A literal string.** Code letters are concatenated and separated by
///   newlines, after an optional run of the `*`, `@` and `^` modifiers, which
///   supply nothing. Verified: `"p\n"` on a one-argument command calls
///   cleanly, so a trailing empty segment supplies nothing, and `""` supplies
///   nothing at all.
/// - **Anything else.** A non-string descriptor is *evaluated* to get the
///   argument list, so `(interactive (list a b))` supplies whatever that
///   returns. Reading it would be a guess, and a guess in this direction is a
///   false positive on correct code.
pub(crate) fn interactive_argument_count(header: &ExpressionView) -> Option<usize> {
    let Some(descriptor) = header.children.get(1) else {
        return Some(0);
    };
    let body = string_literal_body(descriptor)?;
    let letters = body.trim_start_matches(['*', '@', '^']);

    // The reader turns the two source characters `\n` into one newline; the
    // spec is read from the source, so the escape is what separates segments.
    let mut segments: Vec<&str> = letters.split("\\n").collect();
    while segments.last() == Some(&"") {
        segments.pop();
    }
    // An empty segment anywhere but the end is a spec this cannot read the
    // arity of. Assuming it supplies nothing would invent an argument the
    // command may well receive.
    if segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    Some(segments.len())
}
