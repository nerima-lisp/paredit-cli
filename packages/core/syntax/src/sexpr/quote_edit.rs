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

const QUOTE_OPERATORS: [QuoteOperator; 2] = [
    QuoteOperator {
        prefix: ReaderPrefix::Quote,
        head: "quote",
    },
    QuoteOperator {
        prefix: ReaderPrefix::Function,
        head: "function",
    },
];

impl Edit {
    /// Rewrites the selected quote into `style`'s spelling.
    ///
    /// Already in that spelling is not an error — it is the fixpoint every
    /// caller running this over a whole file wants, and refusing would make
    /// "normalize this file" a matter of knowing which forms already comply.
    /// A form that is not a quote at all *is* refused, because that is a
    /// selector that missed rather than a no-op worth reporting as success.
    pub fn normalize_quotes(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
        style: QuoteStyle,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        match style {
            QuoteStyle::Shorthand => to_shorthand(input, tree, selection),
            QuoteStyle::Longhand => to_longhand(input, tree, selection),
        }
    }
}

/// `(quote x)` becomes `'x`; an already-prefixed form is left as it is.
fn to_shorthand(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
    let node = selection.node();
    if outermost_quote_prefix(node.reader_prefixes.as_slice()).is_some() {
        return Ok(input.to_owned());
    }

    let operator = quote_list_operator(tree, selection).ok_or(StructureError::NotAQuoteForm)?;
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
fn to_longhand(input: &str, tree: &SyntaxTree, selection: Selection<'_>) -> SexprResult<String> {
    let node = selection.node();
    let Some(operator) = outermost_quote_prefix(node.reader_prefixes.as_slice()) else {
        // Not prefixed. Either it is already the list form this produces, or
        // it is not a quote at all.
        return if quote_list_operator(tree, selection).is_some() {
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

/// The quote operator the outermost reader prefix denotes, if it is one.
fn outermost_quote_prefix(prefixes: &[ReaderPrefix]) -> Option<QuoteOperator> {
    let first = *prefixes.first()?;
    QUOTE_OPERATORS
        .into_iter()
        .find(|operator| operator.prefix == first)
}

/// The quote operator this list's head names, if it is one.
///
/// Case-insensitive because the Common Lisp reader upcases an unescaped
/// symbol, so `(QUOTE x)` and `(quote x)` are the same form.
fn quote_list_operator(tree: &SyntaxTree, selection: Selection<'_>) -> Option<QuoteOperator> {
    let node = selection.node();
    if node.kind != NodeKind::List {
        return None;
    }
    let head = tree.node(*node.children.first()?);
    if head.kind != NodeKind::Atom {
        return None;
    }
    let text = head.span.slice(&tree.source);
    QUOTE_OPERATORS
        .into_iter()
        .find(|operator| text.eq_ignore_ascii_case(operator.head))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sexpr::ExpressionPath;

    fn apply(source: &str, path: &str, style: QuoteStyle) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        let selection = tree
            .select_path(&path.parse::<ExpressionPath>().unwrap())
            .unwrap();
        Edit::normalize_quotes(source, &tree, selection, style)
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
        let tree = SyntaxTree::parse(&longhand).unwrap();
        let selection = tree
            .select_path(&"0.1".parse::<ExpressionPath>().unwrap())
            .unwrap();
        assert_eq!(
            Edit::normalize_quotes(&longhand, &tree, selection, QuoteStyle::Shorthand).unwrap(),
            source
        );
    }
}
