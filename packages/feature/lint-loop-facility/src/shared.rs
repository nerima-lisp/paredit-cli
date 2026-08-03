//! What the rules in this crate share below the `loop` grammar itself: how to
//! read an atom, how to compare a symbol the way Common Lisp's reader does, and
//! how to tell code from data.
//!
//! Nothing here runs per visited node. Every helper is called from inside a
//! rule that has already matched the `loop` head, and the expensive one
//! ([`is_unevaluated_at`]) is called only once a finding is otherwise ready to
//! report — it descends from [`SyntaxTree::root_view`], which materializes the
//! whole document, so calling it before the cheap grammar check would cost four
//! orders of magnitude more per invocation than the check it guards. A sibling
//! batch measured exactly that inversion at 450843 ns/call against 28.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::unqualified;

/// An atom's text, exactly as the source spells it — *including* any reader
/// prefix.
///
/// That is what makes this safe to compare against a bare symbol name without
/// also testing `reader_prefixes`: `'collect` has `text == "'collect"`, which
/// is not equal to `"collect"`, so quoted data can never be read as a clause
/// keyword.
#[must_use]
pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The exact head symbol of a `(...)` list, or `None` for anything else.
#[must_use]
pub fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren))
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

/// One bare symbol's name, folded the way Common Lisp's reader folds it:
/// package qualifier dropped, ASCII-lowercased.
///
/// A reader prefix disqualifies the atom outright. `'collect` is the *symbol*
/// `collect` passed as data and `#'count` is a function object; neither is the
/// clause keyword it is spelled like. A string literal keeps its quotes in
/// `text` and a number its digits, so neither can collide with a symbol.
#[must_use]
pub fn symbol_word(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    if text.is_empty() || text.starts_with('"') {
        return None;
    }
    // A token that reads as a number is not a symbol. `1` and `-1` cannot name
    // a variable, and letting them through would let `for 1 = …` invent one.
    if text
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        return None;
    }
    Some(unqualified(text).to_ascii_lowercase())
}

/// Whether `view` is a `(head …)` call to `name`, unquoted, comparing the head
/// the way the reader does.
#[must_use]
pub fn is_call_to(view: &ExpressionView, name: &str) -> bool {
    view.reader_prefixes.is_empty()
        && list_head(view).is_some_and(|head| unqualified(head).eq_ignore_ascii_case(name))
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
/// This matters more here than for most rules: a `loop` written inside a
/// `defmacro` template is the *expansion's* code, not this macro's, and the
/// expansion is never seen. Every rule in this crate therefore refuses a
/// `loop` reached only as data.
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
    /// `#`, `#'` and `#.` are deliberately neutral: none of them turns code
    /// into data. `#.` is read-time *evaluation*, and `#(1 2 3)` is a literal
    /// vector whose elements are still read — treating either as a quote would
    /// silence every rule under one.
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

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| unqualified(head).eq_ignore_ascii_case("quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size —
/// but the [`SyntaxTree::root_view`] it starts from materializes the whole
/// document, which is what makes it expensive relative to a shape test. Every
/// caller applies its own cheap grammar check first and reaches this only with
/// a finding otherwise ready to report.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// quasiquoted does not settle it: `` `(a ,(loop …)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
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

/// One symbol's name with any reader prefix stripped off first.
///
/// [`symbol_word`] refuses a prefixed atom outright, which is right when asking
/// "is this token a clause keyword?" — `'collect` is not. It is wrong when
/// asking "is this variable mentioned anywhere?", because the reader spells a
/// spliced reference `,@acc` as a single atom whose `text` is `",@acc"`. That
/// atom **does** read the variable.
fn symbol_word_ignoring_prefix(view: &ExpressionView) -> Option<String> {
    let mut text = atom_text(view)?;
    loop {
        let stripped = text
            .strip_prefix(",@")
            .or_else(|| text.strip_prefix("#'"))
            .or_else(|| text.strip_prefix(','))
            .or_else(|| text.strip_prefix('\''))
            .or_else(|| text.strip_prefix('`'));
        match stripped {
            Some(rest) => text = rest,
            None => break,
        }
    }
    if text.is_empty() || text.starts_with('"') {
        return None;
    }
    if text
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        return None;
    }
    Some(unqualified(text).to_ascii_lowercase())
}

/// How many times the variable `name` is *read* anywhere in `view`, under the
/// two-counter quote model.
///
/// A mention inside a hard `'` is the symbol, not the variable, and does not
/// count. A mention inside a `` ` `` template does not count either — until an
/// unquote escapes back to code, at which point it does.
///
/// That last case is not a corner: it is the single most common way a Common
/// Lisp macro reads a `loop` accumulator.
///
/// ```text
/// (loop for i in funs collect `(defun ,i ()) into defines
///       finally (return `(progn ,@defines)))
/// ```
///
/// `defines` is read exactly once, by the `,@defines`. An earlier version of
/// this function returned `0` for any node carrying a reader prefix and so
/// never descended into the `finally`'s template — which made
/// `loop-into-accumulator-never-read` report 41 findings over SBCL's own
/// sources, **every one of them a false positive** on this shape.
#[must_use]
pub fn count_evaluated_reads(view: &ExpressionView, name: &str) -> usize {
    fn walk(view: &ExpressionView, name: &str, outer: QuoteState, quoting: bool) -> usize {
        let mut state = outer.after_prefixes(view);
        if quoting {
            state = state.quoted();
        }
        if view.kind == ExpressionKind::Atom {
            let matches = symbol_word_ignoring_prefix(view).is_some_and(|word| word == name);
            return usize::from(matches && !state.is_data());
        }
        let quoting = is_quote_form(view);
        view.children
            .iter()
            .map(|child| walk(child, name, state, quoting))
            .sum()
    }
    walk(view, name, QuoteState::EVALUATED, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn parse(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse")
    }

    fn first_form(tree: &SyntaxTree) -> ExpressionView {
        tree.select_path(&SexprPath::root_child(0))
            .expect("root form")
            .view()
    }

    #[test]
    fn a_package_qualifier_and_case_both_fold() {
        let tree = parse("(cl:COLLECT)");
        let view = first_form(&tree);
        assert_eq!(symbol_word(&view.children[0]).as_deref(), Some("collect"));
    }

    #[test]
    fn a_reader_prefix_disqualifies_a_symbol() {
        let tree = parse("(f 'collect #'count `x ,y)");
        let view = first_form(&tree);
        for child in &view.children[1..] {
            assert_eq!(
                symbol_word(child),
                None,
                "{:?} read as a symbol",
                child.text
            );
        }
    }

    #[test]
    fn a_string_or_number_is_not_a_symbol() {
        let tree = parse("(f \"collect\" 1 42)");
        let view = first_form(&tree);
        for child in &view.children[1..] {
            assert_eq!(
                symbol_word(child),
                None,
                "{:?} read as a symbol",
                child.text
            );
        }
    }

    /// The distinction a single depth counter cannot express: a comma inside a
    /// quasiquote escapes back to code, a comma inside a hard quote does not.
    #[test]
    fn an_unquote_escapes_a_quasiquote_but_not_a_quote() {
        let tree = parse("(defmacro m () `(progn ,(loop for x in items collect x)))");
        let root = tree.root_view();
        let target = find_loop(&root).expect("loop present");
        assert!(!is_unevaluated_at(&tree, target));

        let tree = parse("(defparameter *a* '(progn (loop for x in items collect x)))");
        let root = tree.root_view();
        let target = find_loop(&root).expect("loop present");
        assert!(is_unevaluated_at(&tree, target));
    }

    #[test]
    fn a_macro_template_loop_is_data() {
        let tree = parse("(defmacro m (&body body) `(progn (loop for x in z collect x) ,@body))");
        let root = tree.root_view();
        let target = find_loop(&root).expect("loop present");
        assert!(is_unevaluated_at(&tree, target));
    }

    #[test]
    fn a_plain_loop_is_code() {
        let tree = parse("(defun f () (loop for x in items collect x))");
        let root = tree.root_view();
        let target = find_loop(&root).expect("loop present");
        assert!(!is_unevaluated_at(&tree, target));
    }

    /// The long-hand spelling the reader never produces but macro output does.
    #[test]
    fn a_long_hand_quote_form_is_data() {
        let tree = parse("(defparameter *a* (quote (loop for x in z collect x)))");
        let root = tree.root_view();
        let target = find_loop(&root).expect("loop present");
        assert!(is_unevaluated_at(&tree, target));
    }

    fn find_loop(view: &ExpressionView) -> Option<ByteSpan> {
        if list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("loop")) {
            return Some(view.span);
        }
        view.children.iter().find_map(find_loop)
    }
}
