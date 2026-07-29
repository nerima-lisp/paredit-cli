//! Converting between a quote's two spellings.
//!
//! `'x` and `(quote x)` denote the same thing, and so do `#'f` and
//! `(function f)`: the reader expands the prefix into the list form before
//! anything else sees it. Which one a file uses is a style choice, and one
//! that tends to drift — a macro that emits `(quote ...)` sits beside
//! hand-written `'...` in the same file, and neither `format` nor any lint
//! rule reconciles them, because both are correct.
//!
//! Only these two pairs are handled. Quasiquote and unquote are deliberately
//! absent: `` ` ``, `,` and `,@` have no portable list spelling — the
//! standard does not name the operators they expand to, and implementations
//! disagree (`system::quasiquote`, `sb-int:quasiquote`, …), so "normalizing"
//! them would mean picking one implementation's internals and writing them
//! into the file.

use super::edit::{Edit, replace_span, validate_edit_context};
use super::error::{SexprResult, StructureError};
use super::reader_prefix_edit::content_start;
use super::tree::{NodeKind, ReaderPrefix, Selection, SyntaxTree};
use crate::dialect::Dialect;

/// Which of the two spellings to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    /// The reader prefix: `'x`, `#'f`.
    Shorthand,
    /// The list form the reader expands it into: `(quote x)`, `(function f)`.
    Longhand,
}

/// A quote-family operator, in both of its spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteOperator {
    prefix: ReaderPrefix,
    head: &'static str,
}

const QUOTE_OPERATOR: QuoteOperator = QuoteOperator {
    prefix: ReaderPrefix::Quote,
    head: "quote",
};

const FUNCTION_OPERATOR: QuoteOperator = QuoteOperator {
    prefix: ReaderPrefix::Function,
    head: "function",
};

/// Every dialect: `'x` and `(quote x)` are the same form everywhere this tool
/// reads.
const UNIVERSAL_QUOTE_OPERATORS: [QuoteOperator; 1] = [QUOTE_OPERATOR];

/// The Common Lisp family, which additionally has `#'f` / `(function f)`.
const COMMON_LISP_QUOTE_OPERATORS: [QuoteOperator; 2] = [QUOTE_OPERATOR, FUNCTION_OPERATOR];

/// The quote operators `dialect` spells both ways.
///
/// The `function` pair is confined to Common Lisp and Emacs Lisp because
/// [`ReaderPrefix::Function`] is *not* "the `#'` reader macro". It is the
/// parser's catch-all single-prefix slot, and `reader_policy` hands it to
/// Clojure's `@x` (deref), Janet's `|x` (short fn) and Fennel's `#x`
/// (hashfn); even where `#'` itself is the spelling, Scheme and Racket read
/// it as `syntax`, not `function`. `(function f)` is a form only these two
/// dialects have, which is the same pair
/// `unused_parameter_report::plan_ignore_declarations` gates `declare` behind.
///
/// The `quote` pair stays universal because it genuinely is: every dialect
/// here expands `'x` into `(quote x)`.
const fn quote_operators(dialect: Dialect) -> &'static [QuoteOperator] {
    match dialect {
        Dialect::CommonLisp | Dialect::EmacsLisp => &COMMON_LISP_QUOTE_OPERATORS,
        _ => &UNIVERSAL_QUOTE_OPERATORS,
    }
}

impl Edit {
    /// Rewrites the selected quote into `style`'s spelling.
    ///
    /// Already in that spelling is not an error — it is the fixpoint every
    /// caller running this over a whole file wants, and refusing would make
    /// "normalize this file" a matter of knowing which forms already comply.
    /// A form that is not a quote at all *is* refused, because that is a
    /// selector that missed rather than a no-op worth reporting as success.
    ///
    /// `dialect` decides which operators count: see [`quote_operators`].
    pub fn normalize_quotes(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        style: QuoteStyle,
        dialect: Dialect,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        match style {
            QuoteStyle::Shorthand => to_shorthand(input, tree, selection, dialect),
            QuoteStyle::Longhand => to_longhand(input, tree, selection, dialect),
        }
    }
}

/// `(quote x)` becomes `'x`; an already-prefixed form is left as it is.
fn to_shorthand(
    input: &str,
    tree: &SyntaxTree,
    selection: Selection<'_>,
    dialect: Dialect,
) -> SexprResult<String> {
    let node = selection.node();
    // A prefixed form never reaches the list branch below. `node.span` covers
    // the reader-prefix bytes, so `replace_span(input, node.span, "'x")` would
    // delete the prefix along with the list: `` `(quote x) `` became `'x`, and
    // `,(quote x)` became `'x` too — an inversion of when the form is
    // evaluated, in output that still reparses and so slips past `--write`'s
    // reparse guard.
    //
    // Refusal, rather than rewriting only `content_start(node)..span.end()`
    // and letting the prefix survive: keeping the prefix and shortening the
    // list inside it is right for `` ` ``, `,` and `,@`, which leave a list a
    // list, but wrong for `#(`, where `#(quote x)` is a two-element vector
    // literal and `#'x` would read as `(function x)` — the same class of
    // corruption, in the other direction. Telling those apart needs a
    // per-dialect taxonomy of which prefixes are transparent, and
    // `ReaderPrefix` is already dialect-overloaded (see [`quote_operators`]).
    // A form under a non-quote prefix is not a quote form, and is reported as
    // one.
    if !node.reader_prefixes.is_empty() {
        return if outermost_quote_prefix(dialect, node.reader_prefixes.as_slice()).is_some() {
            Ok(input.to_owned())
        } else {
            Err(StructureError::NotAQuoteForm.into())
        };
    }

    let operator =
        quote_list_operator(dialect, tree, selection).ok_or(StructureError::NotAQuoteForm)?;
    // A quote list has exactly two children: the head and the datum it quotes.
    // `(quote)` and `(quote a b)` are malformed rather than shortenable, and
    // silently dropping the extra datum would be a rewrite nobody asked for.
    if node.children.len() != 2 {
        return Err(StructureError::NotAQuoteForm.into());
    }
    let datum_id = *node.children.get(1).ok_or(StructureError::NotAQuoteForm)?;

    let datum = tree.node(datum_id);
    let quoted = datum.span.slice(&tree.source);
    Ok(replace_span(
        input,
        node.span,
        &format!("{}{quoted}", operator.prefix.as_source()),
    ))
}

/// `'x` becomes `(quote x)`; an already-listed form is left as it is.
fn to_longhand(
    input: &str,
    tree: &SyntaxTree,
    selection: Selection<'_>,
    dialect: Dialect,
) -> SexprResult<String> {
    let node = selection.node();
    let Some(operator) = outermost_quote_prefix(dialect, node.reader_prefixes.as_slice()) else {
        // Not a quote prefix. Either it is already the list form this produces
        // (possibly under a quasiquote, which is left where it is), or it is
        // not a quote at all.
        return if quote_list_operator(dialect, tree, selection).is_some() {
            Ok(input.to_owned())
        } else {
            Err(StructureError::NotAQuoteForm.into())
        };
    };

    // Only the outermost prefix is expanded, matching `unwrap_prefix`'s own
    // rule: `'#'f` becomes `(quote #'f)`, not `(quote (function f))`. Peeling
    // one layer per call is what makes repeated calls predictable.
    let prefix_end = if node.reader_prefixes.len() == 1 {
        content_start(node).get()
    } else {
        node.reader_prefix_spans[1].start().get()
    };
    let quoted = &input[prefix_end..node.span.end().get()];
    Ok(replace_span(
        input,
        node.span,
        &format!("({} {quoted})", operator.head),
    ))
}

/// The quote operator the outermost reader prefix denotes, if it is one in
/// this dialect.
fn outermost_quote_prefix(dialect: Dialect, prefixes: &[ReaderPrefix]) -> Option<QuoteOperator> {
    let first = *prefixes.first()?;
    quote_operators(dialect)
        .iter()
        .copied()
        .find(|operator| operator.prefix == first)
}

/// The quote operator this list's head names, if it is one in this dialect.
///
/// Case-insensitive because the Common Lisp reader upcases an unescaped
/// symbol, so `(QUOTE x)` and `(quote x)` are the same form.
fn quote_list_operator(
    dialect: Dialect,
    tree: &SyntaxTree,
    selection: Selection<'_>,
) -> Option<QuoteOperator> {
    let node = selection.node();
    if node.kind != NodeKind::List {
        return None;
    }
    let head = tree.node(*node.children.first()?);
    if head.kind != NodeKind::Atom {
        return None;
    }
    let text = head.span.slice(&tree.source);
    quote_operators(dialect)
        .iter()
        .copied()
        .find(|operator| text.eq_ignore_ascii_case(operator.head))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sexpr::ExpressionPath;

    fn apply_in(
        source: &str,
        path: &str,
        style: QuoteStyle,
        dialect: Dialect,
    ) -> SexprResult<String> {
        // The dialect reader, not the legacy one: the CLI parses with the
        // dialect it then edits with, and the two readers disagree about what
        // a reader prefix even is.
        let tree = SyntaxTree::parse_with_dialect(source, dialect).unwrap();
        let selection = tree
            .select_path(&path.parse::<ExpressionPath>().unwrap())
            .unwrap();
        Edit::normalize_quotes(source, &tree, selection, style, dialect)
    }

    fn apply(source: &str, path: &str, style: QuoteStyle) -> SexprResult<String> {
        apply_in(source, path, style, Dialect::CommonLisp)
    }

    #[test]
    fn quote_list_becomes_a_prefix() {
        assert_eq!(
            apply("(list (quote x) y)", "0.1", QuoteStyle::Shorthand).unwrap(),
            "(list 'x y)"
        );
    }

    #[test]
    fn function_list_becomes_a_sharp_quote() {
        assert_eq!(
            apply("(mapcar (function car) xs)", "0.1", QuoteStyle::Shorthand).unwrap(),
            "(mapcar #'car xs)"
        );
    }

    #[test]
    fn a_quote_prefix_becomes_a_list() {
        assert_eq!(
            apply("(list 'x y)", "0.1", QuoteStyle::Longhand).unwrap(),
            "(list (quote x) y)"
        );
    }

    #[test]
    fn a_sharp_quote_becomes_a_function_list() {
        assert_eq!(
            apply("(mapcar #'car xs)", "0.1", QuoteStyle::Longhand).unwrap(),
            "(mapcar (function car) xs)"
        );
    }

    #[test]
    fn a_quoted_list_keeps_its_contents() {
        assert_eq!(
            apply("(list (quote (a b c)) y)", "0.1", QuoteStyle::Shorthand).unwrap(),
            "(list '(a b c) y)"
        );
        assert_eq!(
            apply("(list '(a b c) y)", "0.1", QuoteStyle::Longhand).unwrap(),
            "(list (quote (a b c)) y)"
        );
    }

    #[test]
    fn the_head_is_matched_case_insensitively() {
        assert_eq!(
            apply("(list (QUOTE x))", "0.1", QuoteStyle::Shorthand).unwrap(),
            "(list 'x)"
        );
    }

    #[test]
    fn a_form_already_in_the_requested_style_is_unchanged() {
        assert_eq!(
            apply("(list 'x)", "0.1", QuoteStyle::Shorthand).unwrap(),
            "(list 'x)"
        );
        assert_eq!(
            apply("(list (quote x))", "0.1", QuoteStyle::Longhand).unwrap(),
            "(list (quote x))"
        );
    }

    #[test]
    fn only_the_outermost_prefix_is_expanded() {
        assert_eq!(
            apply("(list '#'f)", "0.1", QuoteStyle::Longhand).unwrap(),
            "(list (quote #'f))"
        );
    }

    #[test]
    fn a_form_that_is_not_a_quote_is_refused() {
        for style in [QuoteStyle::Shorthand, QuoteStyle::Longhand] {
            let error = apply("(list (f x))", "0.1", style).unwrap_err();
            assert!(error.to_string().contains("not a quote"), "{error}");
        }
    }

    #[test]
    fn a_malformed_quote_list_is_refused_rather_than_truncated() {
        // `(quote a b)` is not a quote of `a` with `b` discarded.
        let error = apply("(list (quote a b))", "0.1", QuoteStyle::Shorthand).unwrap_err();
        assert!(error.to_string().contains("not a quote"), "{error}");
        let error = apply("(list (quote))", "0.1", QuoteStyle::Shorthand).unwrap_err();
        assert!(error.to_string().contains("not a quote"), "{error}");
    }

    #[test]
    fn the_two_directions_round_trip() {
        let source = "(mapcar #'car xs)";
        let longhand = apply(source, "0.1", QuoteStyle::Longhand).unwrap();
        assert_eq!(longhand, "(mapcar (function car) xs)");
        assert_eq!(
            apply(&longhand, "0.1", QuoteStyle::Shorthand).unwrap(),
            source
        );
    }

    #[test]
    fn a_non_quote_reader_prefix_is_refused_rather_than_deleted() {
        // `node.span` includes the prefix bytes, so the list branch used to
        // replace the prefix along with the list. Every one of these produced
        // `(list 'x y)`, output that still reparses and so never tripped the
        // `--write` reparse guard; `,`'s loss inverts when the form runs, and
        // `#(quote x)` is a vector literal rather than a quote form at all.
        for source in [
            "(list `(quote x) y)",
            "(list ,(quote x) y)",
            "(list ,@(quote x) y)",
            "(list #(quote x) y)",
        ] {
            let error = apply(source, "0.1", QuoteStyle::Shorthand).unwrap_err();
            assert!(
                error.to_string().contains("not a quote"),
                "{source}: {error}"
            );
        }
    }

    #[test]
    fn a_quasiquoted_quote_list_is_already_longhand() {
        // The other direction has nothing to rewrite: the list inside the
        // prefix is the form this style produces.
        assert_eq!(
            apply("(list `(quote x) y)", "0.1", QuoteStyle::Longhand).unwrap(),
            "(list `(quote x) y)"
        );
    }

    #[test]
    fn the_quote_pair_is_recognised_in_every_dialect() {
        for dialect in [
            Dialect::CommonLisp,
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Fennel,
            Dialect::Lfe,
            Dialect::Hy,
            Dialect::Carp,
            Dialect::Janet,
        ] {
            assert_eq!(
                apply_in("(list (quote x) y)", "0.1", QuoteStyle::Shorthand, dialect).unwrap(),
                "(list 'x y)",
                "{dialect:?}"
            );
        }
    }

    #[test]
    fn the_function_pair_is_confined_to_the_common_lisp_family() {
        // `ReaderPrefix::Function` is the parser's catch-all prefix slot, not
        // the `#'` reader macro: `reader_policy` gives it Clojure's `@x`
        // (deref) and Fennel's `#x` (hashfn) as well. `(map #'inc xs)` in
        // Clojure used to expand to `(map (function inc) xs)`, and `function`
        // is not a Clojure form.
        for (dialect, source) in [
            (Dialect::Clojure, "(map #'inc xs)"),
            (Dialect::Clojure, "(map @xs ys)"),
            (Dialect::Fennel, "(map #x ys)"),
        ] {
            let error = apply_in(source, "0.1", QuoteStyle::Longhand, dialect).unwrap_err();
            assert!(
                error.to_string().contains("not a quote"),
                "{dialect:?} {source}: {error}"
            );
        }

        // And the list spelling is not shortened back into a prefix either.
        let error = apply_in(
            "(map (function inc) xs)",
            "0.1",
            QuoteStyle::Shorthand,
            Dialect::Clojure,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not a quote"), "{error}");
    }
}
