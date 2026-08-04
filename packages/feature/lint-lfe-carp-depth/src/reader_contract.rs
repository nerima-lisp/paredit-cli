//! What both rules assume this workspace's reader produces for LFE.
//!
//! Every assertion here was first *printed* rather than guessed, and each one
//! is load-bearing: if the reader changes any of these, a rule in this package
//! becomes silently wrong rather than failing. The `#B(…)` case in particular
//! was broken until recently — PR #98 fixed `#B(…)`/`#M(…)` being orphaned
//! from their enclosing list in 243 of 2701 files, which silently inflated
//! arity — so the shape is pinned rather than trusted.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};

fn parse(source: &str) -> ExpressionView {
    SyntaxTree::parse_with_dialect(source, Dialect::Lfe)
        .expect("parse")
        .root_view()
        .children
        .first()
        .expect("one top-level form")
        .clone()
}

/// A module-qualified call keeps its colon *inside the head atom*, rather than
/// being split into separate nodes.
///
/// `illegal_guard_call` does a text split on this. If the reader ever split
/// `lists:member` into three tokens, the rule would stop finding anything.
#[test]
fn a_qualified_name_is_one_atom_with_the_colon_in_its_text() {
    let form = parse("(lists:member x y)");
    let head = form.children.first().expect("head");
    assert_eq!(head.kind, ExpressionKind::Atom);
    assert_eq!(head.text.as_deref(), Some("lists:member"));
    assert!(head.reader_prefixes.is_empty());
}

/// A quoted atom carries **both** a `Quote` prefix and the `'` in its text.
///
/// `is_variable_atom` relies on the text half (a `'` is not alphabetic) and
/// `literal_atom` relies on the prefix half. Losing either would change what
/// counts as a pattern variable, which is the whole basis of the clause rule.
#[test]
fn a_quoted_atom_keeps_the_quote_in_both_places() {
    let form = parse("(case x ('one 1))");
    let clause = form.children.get(2).expect("clause");
    let pattern = clause.children.first().expect("pattern");
    assert_eq!(pattern.text.as_deref(), Some("'one"));
    assert_eq!(pattern.reader_prefixes.as_slice(), &[ReaderPrefix::Quote]);
}

/// Every atom-level reader prefix LFE has is included in `text`.
///
/// This is what makes `is_variable_atom`'s first-byte test sufficient, and it
/// is *not* true of every dialect — the same reader strips Carp's `@` sigil
/// from the text. Pinned so the difference stays visible.
#[test]
fn every_atom_prefix_is_included_in_the_text() {
    let form = parse("(case x ('a 1) (,b 2) (`c 3) (,@d 4))");
    let expected = [
        ("'a", ReaderPrefix::Quote),
        (",b", ReaderPrefix::Unquote),
        ("`c", ReaderPrefix::Quasiquote),
        (",@d", ReaderPrefix::UnquoteSplicing),
    ];
    for (index, (text, prefix)) in expected.into_iter().enumerate() {
        let pattern = form
            .children
            .get(index + 2)
            .expect("clause")
            .children
            .first()
            .expect("pattern");
        assert_eq!(pattern.text.as_deref(), Some(text));
        assert_eq!(pattern.reader_prefixes.as_slice(), &[prefix]);
    }
}

/// `#B(…)` and `#M(…)` nest as children of their enclosing list rather than
/// being orphaned beside it, and they are *prefixes* rather than heads.
///
/// This is the PR #98 fix. Before it, `(list #B(1 2))` read as a `list` call
/// with an extra sibling, inflating arity. Neither rule here counts arity, but
/// the clause rule walks children and would see a stray node as a clause.
#[test]
fn lfe_binary_and_map_literals_nest_under_their_enclosing_list() {
    let form = parse("(list #B(1 2) #M(a 1))");
    assert_eq!(form.children.len(), 3, "head plus exactly two literals");
    let binary = form.children.get(1).expect("binary");
    assert_eq!(binary.kind, ExpressionKind::List);
    assert_eq!(
        binary.reader_prefixes.as_slice(),
        &[ReaderPrefix::LfeBinary]
    );
    assert_eq!(binary.children.len(), 2);
    let map = form.children.get(2).expect("map");
    assert_eq!(map.reader_prefixes.as_slice(), &[ReaderPrefix::LfeMap]);
    assert_eq!(map.children.len(), 2);
}

/// A bar-quoted atom is a single atom whose text keeps the bars, so a colon
/// inside it is part of the name.
#[test]
fn a_bar_quoted_atom_is_one_atom_including_the_bars() {
    let form = parse("(|foo bar| x)");
    let head = form.children.first().expect("head");
    assert_eq!(head.kind, ExpressionKind::Atom);
    assert_eq!(head.text.as_deref(), Some("|foo bar|"));
}

/// LFE brackets are a distinct delimiter, which is what `head_symbol`'s
/// paren-only test keys on.
#[test]
fn brackets_are_a_distinct_delimiter() {
    let form = parse("[case x ('one 1)]");
    assert_eq!(form.kind, ExpressionKind::List);
    assert_eq!(form.delimiter, Some(Delimiter::Bracket));
    let parens = parse("(case x ('one 1))");
    assert_eq!(parens.delimiter, Some(Delimiter::Paren));
}

/// A guard sits inside a clause as `(Pattern (when …) . Body)`, which is the
/// position `clause_guard` reads.
#[test]
fn a_guard_is_the_second_element_of_its_clause() {
    let form = parse("(defun a ((x) (when (is_atom x)) 'ok))");
    let clause = form.children.get(2).expect("clause");
    let guard = clause.children.get(1).expect("guard");
    assert_eq!(
        guard.children.first().and_then(|head| head.text.as_deref()),
        Some("when")
    );
}
