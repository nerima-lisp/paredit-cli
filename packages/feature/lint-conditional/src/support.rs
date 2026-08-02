//! What the conditional rules share: which parts of a file are *code*, and how
//! to read a literal that a `case` key or a `cond` test compares against.
//!
//! # Evaluation context
//!
//! The lint engine's dispatch walks into quoted data like any other subtree and
//! [`RuleContext`] carries no parent pointer, so a head-matched `(case …)` node
//! cannot tell on its own whether it is code or a list of symbols inside
//! `'(…)`. [`is_unevaluated_at`] answers that by descending from the root along
//! the single chain of nodes whose span contains the candidate's — depth-many
//! steps, not tree-many — and every rule here calls it *only once it already
//! has a finding to report*.
//!
//! The two counters in [`QuoteState`] are not interchangeable and a single
//! `i32` depth counter is wrong for both: a comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears, while a comma inside
//! `` `(…) `` escapes back to code, so `quasi` counts up and down. A node one
//! level *inside* a quote is still data, so a node-local `reader_prefixes`
//! check is not enough either. These semantics and their tests are copied from
//! `paredit-feature-lint-condition-system`'s `support` rather than depended on:
//! a feature package does not import another feature package.
//!
//! Nothing here is called per visited node. That is deliberate: the
//! `clean/forms/*` benchmarks lint files with zero findings, so the per-file
//! cost of a rule that matches nothing is exactly what they measure.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
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
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none
    /// of them turns code into data.
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
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
///
/// Iterative rather than recursive, so a deeply nested document costs heap
/// rather than stack.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for child in view.children.iter().rev() {
            stack.push((child, inside));
        }
    }
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(case x (1 2))) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
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

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    unqualified(text).to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

/// A bare symbol: an unprefixed atom that reads as a *symbol* and not as a
/// number, string, character or keyword.
///
/// Both halves matter, and getting either wrong silently breaks every caller
/// that asks "is this the variable being dispatched on?":
///
/// - The prefix check keeps `'x` and `#'x` out. A `cond` test comparing against
///   `'x` compares against a *quoted symbol*, which is a literal key rather
///   than a variable.
/// - The [`LiteralKind::Symbol`] check keeps `1` out. Reading an atom's text
///   and calling the result a symbol makes `(eql op 1)` look like a comparison
///   between two variables, so nothing is ever reported.
#[must_use]
pub fn bare_symbol(view: &ExpressionView) -> Option<String> {
    if literal_kind(view) == LiteralKind::Symbol {
        symbol_name(view)
    } else {
        None
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled shape and is left alone throughout.
#[must_use]
pub fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether any direct child of `view` is a reader conditional.
#[must_use]
pub fn has_reader_conditional_child(view: &ExpressionView) -> bool {
    view.children.iter().any(is_reader_conditional)
}

/// What kind of object a `case` key or a `cond` comparison operand denotes,
/// as far as the reader settles it without evaluating anything.
///
/// Only the distinctions the rules here actually turn on. Anything else — a
/// variable, a call, a vector — is [`LiteralKind::NotALiteral`], and every rule
/// declines rather than guesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    /// `"text"`. Two `eql` string objects are the same object or nothing; CLHS
    /// shows `(eql "Foo" "Foo")` as *true or false* depending on the
    /// implementation, and `(eql "Foo" (copy-seq "Foo"))` as false.
    String,
    /// `1.0`, `.5`, `1e5`, `1.0d0`. `eql` is true of two floats of the same
    /// type and value, but the *type* a literal reads as is decided by
    /// `*read-default-float-format*`, and CLHS notes `(eql 1.0s0 1.0d0)` is
    /// normally false.
    Float,
    /// `1`, `-3`, `#xFF` — including bignums, for which `eql` is fully
    /// specified ("both numbers of the same type and the same value").
    Integer,
    /// `1/2`.
    Ratio,
    /// `#\a`. `eql` is specified for characters representing the same
    /// character.
    Character,
    /// `:keyword`.
    Keyword,
    /// A bare symbol, including `t` and `nil`.
    Symbol,
    /// `'foo` or `(quote foo)`.
    QuotedSymbol,
    /// Anything that is not a literal the rules here can read.
    NotALiteral,
}

impl LiteralKind {
    /// Whether `case`'s `eql` dispatch matches this kind of key dependably.
    ///
    /// The negative cases are exactly [`Self::String`] and [`Self::Float`]; see
    /// each variant's own note for the CLHS wording behind that.
    #[must_use]
    pub const fn is_eql_dependable(self) -> bool {
        !matches!(self, Self::String | Self::Float)
    }
}

/// Reads what `view` denotes, without evaluating anything.
#[must_use]
pub fn literal_kind(view: &ExpressionView) -> LiteralKind {
    if is_quote_form(view) {
        return match view.children.get(1) {
            Some(quoted) if atom_text(quoted).is_some() => LiteralKind::QuotedSymbol,
            _ => LiteralKind::NotALiteral,
        };
    }
    let Some(text) = atom_text(view) else {
        return LiteralKind::NotALiteral;
    };
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return LiteralKind::QuotedSymbol;
    }
    if !view.reader_prefixes.is_empty() {
        // `#'f`, `#(1 2)`, `#+sbcl x`: not a key this reads.
        return LiteralKind::NotALiteral;
    }
    literal_kind_of_text(text)
}

/// [`literal_kind`] for an atom's already-extracted text.
#[must_use]
pub fn literal_kind_of_text(text: &str) -> LiteralKind {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        return LiteralKind::String;
    }
    if text.starts_with("#\\") {
        return LiteralKind::Character;
    }
    if text.starts_with(':') {
        return LiteralKind::Keyword;
    }
    if is_float_literal(text) {
        return LiteralKind::Float;
    }
    if is_ratio_literal(text) {
        return LiteralKind::Ratio;
    }
    if is_decimal_integer_literal(text) {
        return LiteralKind::Integer;
    }
    if text.is_empty() || text.starts_with('#') {
        return LiteralKind::NotALiteral;
    }
    LiteralKind::Symbol
}

/// Whether `text` reads as a Common Lisp *float*.
///
/// Deliberately conservative, and deliberately not "contains a dot":
///
/// - `1.` is the **integer** 1 (CLHS 2.3.1, decimal-point-terminated), not a
///   float, so a trailing dot with no fraction and no exponent is rejected.
/// - `1+` and `1-` are symbols naming functions, not numbers.
/// - `e5` and `s1` are symbols: an exponent marker needs digits before it.
/// - `1/2` is a ratio.
///
/// Anything with trailing characters the scan does not consume is a symbol.
#[must_use]
pub fn is_float_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut index = 0;

    if index < len && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }

    let integer_start = index;
    while index < len && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let integer_digits = index - integer_start;

    let mut fraction_digits = 0;
    let mut saw_dot = false;
    if index < len && bytes[index] == b'.' {
        saw_dot = true;
        index += 1;
        let fraction_start = index;
        while index < len && bytes[index].is_ascii_digit() {
            index += 1;
        }
        fraction_digits = index - fraction_start;
    }

    let mut saw_exponent = false;
    if index < len && matches!(bytes[index] | 0x20, b'e' | b's' | b'f' | b'd' | b'l') {
        let mut scan = index + 1;
        if scan < len && (bytes[scan] == b'+' || bytes[scan] == b'-') {
            scan += 1;
        }
        let digits_start = scan;
        while scan < len && bytes[scan].is_ascii_digit() {
            scan += 1;
        }
        if scan > digits_start {
            saw_exponent = true;
            index = scan;
        }
    }

    if index != len || (integer_digits == 0 && fraction_digits == 0) {
        return false;
    }
    // `123.` is a decimal-point-terminated integer, not a float.
    if saw_dot && fraction_digits == 0 && !saw_exponent {
        return false;
    }
    saw_exponent || fraction_digits > 0
}

/// Whether `text` reads as `[sign] digits / digits`.
#[must_use]
pub fn is_ratio_literal(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    match body.split_once('/') {
        Some((numerator, denominator)) => {
            !numerator.is_empty()
                && !denominator.is_empty()
                && numerator.bytes().all(|byte| byte.is_ascii_digit())
                && denominator.bytes().all(|byte| byte.is_ascii_digit())
        }
        None => false,
    }
}

/// Whether `text` reads as a decimal integer, with or without CLHS's trailing
/// decimal point.
#[must_use]
pub fn is_decimal_integer_literal(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let body = body.strip_suffix('.').unwrap_or(body);
    !body.is_empty() && body.bytes().all(|byte| byte.is_ascii_digit())
}

/// The clauses of a `cond`/`case`/`when`-family form: every child past the
/// head, or past the head and the test-key for a `case`.
///
/// A borrow of the matched node's own children, so this allocates nothing.
#[must_use]
pub fn clauses_from(view: &ExpressionView, skip: usize) -> &[ExpressionView] {
    view.children.get(skip..).unwrap_or(&[])
}

/// Whether `view` is a `(…)` list that could be a clause at all.
#[must_use]
pub fn is_clause(view: &ExpressionView) -> bool {
    is_paren_list(view) && view.reader_prefixes.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source);
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    // -- the five quote shapes, on the walk ---------------------------------

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(cond ((eql x 1) 2))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(
            evaluated_heads("(quote (cond ((eql x 1) 2)))"),
            vec!["quote"]
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(cond ((eql x 1) 2))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(cond ((eql x 1) 2)))"),
            vec!["cond", "eql"]
        );
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(cond ((eql x 1) 2)))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(cond ((eql x 1) 2))\")"), vec!["f"]);
    }

    // -- the same five shapes, on the span-directed lookup -------------------

    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f (x) (cond ((eql x 1) 2)))",
            "cond"
        ));
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("'(cond ((eql x 1) 2))", "cond"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(quote (cond ((eql x 1) 2)))",
            "cond"
        ));
    }

    #[test]
    fn a_span_under_a_bare_quasiquote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("`(cond ((eql x 1) 2))", "cond"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "`(a ,(cond ((eql x 1) 2)))",
            "cond"
        ));
    }

    /// The shape a single `i32` depth counter reads wrongly.
    #[test]
    fn a_span_under_a_comma_inside_a_hard_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "'(a ,(cond ((eql x 1) 2)))",
            "cond"
        ));
    }

    /// A node one level *inside* a quote is still data, which a node-local
    /// `reader_prefixes` check would miss: the inner `cond` carries no prefix
    /// of its own.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(unevaluated_at_first_head(
            "'(list (cond ((eql x 1) 2)))",
            "cond"
        ));
    }

    // -- literal reading ------------------------------------------------------

    fn kind(source: &str) -> LiteralKind {
        let parsed = tree(source);
        let root = parsed.root_view();
        literal_kind(&root.children[0])
    }

    #[test]
    fn a_string_literal_is_read_as_a_string() {
        assert_eq!(kind("\"foo\""), LiteralKind::String);
    }

    #[test]
    fn float_spellings_are_read_as_floats() {
        for text in ["1.0", ".5", "-2.25", "1e5", "1.0d0", "3.14s0", "+1.5e-3"] {
            assert!(is_float_literal(text), "{text} is a float");
            assert_eq!(literal_kind_of_text(text), LiteralKind::Float, "{text}");
        }
    }

    /// `1.` is the integer 1 (CLHS 2.3.1), and `1+`/`e5` are symbols.
    #[test]
    fn near_float_spellings_are_not_floats() {
        for text in ["1", "1.", "-3.", "1+", "1-", "e5", "s1", "1/2", "abc", "1e"] {
            assert!(!is_float_literal(text), "{text} is not a float");
        }
    }

    #[test]
    fn integers_ratios_characters_and_keywords_are_read_apart() {
        assert_eq!(literal_kind_of_text("1"), LiteralKind::Integer);
        assert_eq!(literal_kind_of_text("1."), LiteralKind::Integer);
        assert_eq!(
            literal_kind_of_text("100000000000000000000"),
            LiteralKind::Integer
        );
        assert_eq!(literal_kind_of_text("1/2"), LiteralKind::Ratio);
        assert_eq!(literal_kind_of_text("#\\a"), LiteralKind::Character);
        assert_eq!(literal_kind_of_text(":foo"), LiteralKind::Keyword);
        assert_eq!(literal_kind_of_text("foo"), LiteralKind::Symbol);
    }

    #[test]
    fn both_quoted_symbol_spellings_are_read_as_quoted_symbols() {
        assert_eq!(kind("'foo"), LiteralKind::QuotedSymbol);
        assert_eq!(kind("(quote foo)"), LiteralKind::QuotedSymbol);
    }

    #[test]
    fn a_call_is_not_a_literal() {
        assert_eq!(kind("(f x)"), LiteralKind::NotALiteral);
    }

    /// The whole point of [`LiteralKind::is_eql_dependable`]: bignums and
    /// characters are fully specified for `eql`, strings and floats are not.
    #[test]
    fn only_strings_and_floats_are_undependable_under_eql() {
        assert!(!LiteralKind::String.is_eql_dependable());
        assert!(!LiteralKind::Float.is_eql_dependable());
        for dependable in [
            LiteralKind::Integer,
            LiteralKind::Ratio,
            LiteralKind::Character,
            LiteralKind::Keyword,
            LiteralKind::Symbol,
            LiteralKind::QuotedSymbol,
        ] {
            assert!(dependable.is_eql_dependable(), "{dependable:?}");
        }
    }

    #[test]
    fn a_bare_symbol_excludes_a_quoted_one() {
        let parsed = tree("(f x 'y)");
        let call = &parsed.root_view().children[0];
        assert_eq!(bare_symbol(&call.children[1]).as_deref(), Some("x"));
        assert_eq!(bare_symbol(&call.children[2]), None);
    }

    /// Reading an atom's text and calling the result a symbol makes
    /// `(eql op 1)` look like a comparison between two *variables*, which
    /// silently stops `cond-to-case-candidate` reporting anything at all.
    #[test]
    fn a_bare_symbol_excludes_every_non_symbol_atom() {
        let parsed = tree("(f 1 1.5 \"s\" :k #\\a 1/2 t nil)");
        let call = &parsed.root_view().children[0];
        for index in 1..=6 {
            assert_eq!(
                bare_symbol(&call.children[index]),
                None,
                "child {index} is not a symbol"
            );
        }
        // `t` and `nil` *are* symbols; it is each rule's own job to treat them
        // as designators.
        assert_eq!(bare_symbol(&call.children[7]).as_deref(), Some("t"));
        assert_eq!(bare_symbol(&call.children[8]).as_deref(), Some("nil"));
    }

    #[test]
    fn a_package_qualified_symbol_normalizes_to_its_name() {
        let parsed = tree("(f app::x)");
        let call = &parsed.root_view().children[0];
        assert_eq!(bare_symbol(&call.children[1]).as_deref(), Some("x"));
    }

    #[test]
    fn a_reader_conditional_child_is_seen() {
        let parsed = tree("(cond (#+sbcl a 1) (t 2))");
        let form = &parsed.root_view().children[0];
        assert!(has_reader_conditional_child(&form.children[1]));
        assert!(!has_reader_conditional_child(&form.children[2]));
    }
}
