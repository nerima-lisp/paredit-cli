//! Helpers every rule in this crate needs.

use paredit_core_syntax::emacs_lisp::EmacsLispOperator;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

/// An atom's text, or `None` when it carries a reader prefix.
///
/// A prefixed atom is never an operator head: reading `#'defcustom` as the
/// head `defcustom` would treat a function designator as a definition form.
pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then_some(view.text.as_deref())
        .flatten()
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
/// The engine hands a rule nodes whose head matched a *normalized* filter,
/// which lowercases. Emacs Lisp does not, so every rule re-resolves the exact
/// text: a `.el` file that defines `Defcustom` must not be linted as though it
/// had written the special form.
pub(crate) fn emacs_lisp_operator(view: &ExpressionView) -> Option<EmacsLispOperator> {
    EmacsLispOperator::from_head(list_head(view)?)
}

/// Whether `keyword` appears in `view`'s children at or after `start`.
///
/// Keyword arguments in a `defcustom`/`defface` tail are `:key value` pairs,
/// so only the even offsets from `start` are keyword positions — a `:type`
/// that happens to be some other option's *value* does not count.
pub(crate) fn has_keyword_argument(view: &ExpressionView, start: usize, keyword: &str) -> bool {
    view.children
        .get(start..)
        .unwrap_or_default()
        .iter()
        .step_by(2)
        .filter_map(atom_text)
        .any(|text| text == keyword)
}
