//! `fennel-each-over-non-iterator` detection: a Fennel `each` handed a literal
//! where an iterator belongs.
//!
//! `(each [k v tbl] …)` compiles to Lua's generic `for k, v in tbl do` —
//! literally that string, emitted at `specials.fnl:670-672` — and Lua's generic
//! `for` *calls* its iterator on every round. A table is not a function, so
//! `(each [k v {:a 1}] …)` raises "attempt to call a table value" the first time
//! the loop runs. The mistake is writing the collection where `(pairs coll)` or
//! `(ipairs coll)` belongs, which is the single most common Fennel beginner
//! error and the reason the reference opens the section with "Runs the body once
//! for each value provided by the iterator. Commonly used with `ipairs` … or
//! `pairs`" (reference.md, "`each` general iteration").
//!
//! # Why only literals
//!
//! `(each [k v coll] …)` where `coll` is a symbol is *indistinguishable* from
//! `(each [k v my-iterator] …)`, and the second is legal and idiomatic — the
//! reference says so in the same paragraph ("can be used with any iterator").
//! Deciding between them needs to know what `coll` holds, which the binding and
//! value tables cannot say for Fennel (they are empty for this dialect). So the
//! rule fires only where the iterator position holds a value that is provably
//! not callable at the point it is written:
//!
//! - a `[…]` sequence literal or a `{…}` table literal — a fresh literal has no
//!   metatable, so it cannot have a `__call`;
//! - a `"…"` or `:…` string literal, or a number.
//!
//! Every one of those is a runtime error with certainty, and nothing else is
//! reported. That makes a false positive impossible rather than unlikely.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SyntaxTree};

use crate::support::{head_symbol, symbol_text};

pub const DIALECTS: [Dialect; 1] = [Dialect::Fennel];

/// The clause keywords that end the driver list rather than drive it.
///
/// `&until` is the current spelling and `:until` the pre-1.2.0 one the parser
/// still accepts (reference.md, "`each` general iteration"). Both are removed
/// before the last driver is read, exactly as `iterator-bindings` does it
/// (`specials.fnl:637-648`).
const CLAUSE_KEYWORDS: [&str; 2] = ["&until", ":until"];

/// What was found in the iterator position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonIterator {
    SequenceLiteral,
    TableLiteral,
    StringLiteral,
    NumberLiteral,
}

impl NonIterator {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::SequenceLiteral => "a sequence literal",
            Self::TableLiteral => "a table literal",
            Self::StringLiteral => "a string literal",
            Self::NumberLiteral => "a number literal",
        }
    }

    /// What to wrap it in. A sequence wants `ipairs`; anything else that is a
    /// collection wants `pairs`.
    #[must_use]
    pub const fn suggestion(self) -> &'static str {
        match self {
            Self::SequenceLiteral => "(ipairs …)",
            _ => "(pairs …)",
        }
    }
}

/// One `each` whose iterator position holds something uncallable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonIteratorEach {
    /// The whole `(each …)` form.
    pub span: ByteSpan,
    /// The offending expression alone.
    pub iterator_span: ByteSpan,
    pub kind: NonIterator,
}

/// Whether an atom's text is a literal rather than a name.
///
/// Fennel string literals are `"…"`, and a `:`-prefixed token is also a string
/// as long as it is symbol-shaped (reference.md, "Syntax": "Fennel also
/// supports certain kinds of strings that begin with a colon"). Numbers are Lua
/// numbers plus `.inf`/`.nan`.
fn literal_kind(text: &str) -> Option<NonIterator> {
    if text.starts_with('"') {
        return Some(NonIterator::StringLiteral);
    }
    if text.starts_with(':') && text.len() > 1 {
        return Some(NonIterator::StringLiteral);
    }
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let numeric = unsigned.starts_with(|byte: char| byte.is_ascii_digit())
        || matches!(unsigned, ".inf" | ".nan");
    numeric.then_some(NonIterator::NumberLiteral)
}

/// Classifies the expression sitting in the iterator position, or `None` when
/// it is anything this rule refuses to judge.
fn classify(view: &ExpressionView) -> Option<NonIterator> {
    match view.kind {
        ExpressionKind::List => match view.delimiter {
            Some(Delimiter::Bracket) => Some(NonIterator::SequenceLiteral),
            Some(Delimiter::Brace) => Some(NonIterator::TableLiteral),
            // A `(…)` call may well return an iterator; that is the correct
            // spelling and the overwhelmingly common one.
            _ => None,
        },
        ExpressionKind::Atom => literal_kind(symbol_text(view)?),
        ExpressionKind::Root => None,
    }
}

/// The expression that drives an `each`, given its binder bracket.
///
/// `iterator-bindings` (`specials.fnl:637-648`) removes the `&until` clause and
/// then takes the *last* remaining item; everything before it is a name. This
/// reproduces that, and returns `None` for a binder too short to have both.
fn iterator_of(binder: &ExpressionView) -> Option<&ExpressionView> {
    let mut drivers: &[ExpressionView] = &binder.children;
    for (index, child) in binder.children.iter().enumerate() {
        let is_clause = symbol_text(child).is_some_and(|text| CLAUSE_KEYWORDS.contains(&text));
        if is_clause {
            drivers = &binder.children[..index];
            break;
        }
    }
    // One name and one iterator is the minimum `each` accepts
    // (`specials.fnl:669`, "expected binding and iterator").
    (drivers.len() >= 2).then(|| drivers.last())?
}

/// Examines one form.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<NonIteratorEach> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    if head_symbol(view) != Some("each") {
        return None;
    }
    // `(each [k v tbl])` with no body is malformed and the compiler says so
    // (`specials.fnl:651`, "expected body expression").
    if view.children.len() < 3 {
        return None;
    }
    let binder = view.children.get(1)?;
    if binder.delimiter != Some(Delimiter::Bracket) {
        return None;
    }
    let iterator = iterator_of(binder)?;
    let kind = classify(iterator)?;
    Some(NonIteratorEach {
        span: view.span,
        iterator_span: iterator.span,
        kind,
    })
}

/// Every offending `each` in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<NonIteratorEach> {
    let root = tree.root_view();
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// Every `(each …)` form in the file, judged or not. The denominator.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view) == Some("each") {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<NonIterator> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Fennel).expect("parse");
        collect(Dialect::Fennel, &tree)
            .into_iter()
            .map(|item| item.kind)
            .collect()
    }

    #[test]
    fn flags_a_table_literal_in_the_iterator_position() {
        assert_eq!(
            kinds("(each [k v {:a 1}] (print k v))"),
            vec![NonIterator::TableLiteral]
        );
    }

    #[test]
    fn flags_a_sequence_literal_and_suggests_ipairs() {
        assert_eq!(
            kinds("(each [i x [1 2 3]] (print x))"),
            vec![NonIterator::SequenceLiteral]
        );
        assert_eq!(NonIterator::SequenceLiteral.suggestion(), "(ipairs …)");
    }

    #[test]
    fn flags_string_and_number_literals() {
        assert_eq!(
            kinds("(each [c \"abc\"] (print c))"),
            vec![NonIterator::StringLiteral]
        );
        assert_eq!(
            kinds("(each [c :abc] (print c))"),
            vec![NonIterator::StringLiteral]
        );
        assert_eq!(
            kinds("(each [n 10] (print n))"),
            vec![NonIterator::NumberLiteral]
        );
    }

    #[test]
    fn the_correct_spellings_are_left_alone() {
        assert!(kinds("(each [k v (pairs t)] (print k v))").is_empty());
        assert!(kinds("(each [i x (ipairs xs)] (print x))").is_empty());
    }

    #[test]
    fn a_bare_symbol_iterator_is_never_judged() {
        // Indistinguishable from a real iterator function, which the
        // reference explicitly permits.
        assert!(kinds("(each [k v coll] (print k v))").is_empty());
        assert!(kinds("(each [line my-iterator] (print line))").is_empty());
    }

    #[test]
    fn the_until_clause_does_not_shift_which_child_is_the_iterator() {
        // Without removing `&until` first, `done?` would be read as the
        // iterator and `(pairs t)` as a name — the finding would be missed,
        // and a literal `&until` bound would be reported.
        assert!(kinds("(each [k v (pairs t) &until done?] (print k))").is_empty());
        assert_eq!(
            kinds("(each [k v {:a 1} &until done?] (print k))"),
            vec![NonIterator::TableLiteral]
        );
        assert!(kinds("(each [k v (pairs t) :until done?] (print k))").is_empty());
    }

    #[test]
    fn a_binder_with_no_room_for_both_is_not_judged() {
        assert!(kinds("(each [{:a 1}] (print 1))").is_empty());
    }

    #[test]
    fn a_malformed_each_is_left_to_the_compiler() {
        assert!(kinds("(each [k v {:a 1}])").is_empty());
        assert!(kinds("(each)").is_empty());
    }

    #[test]
    fn a_non_bracket_binder_is_not_judged() {
        assert!(kinds("(each (k v {:a 1}) (print k))").is_empty());
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // Janet's `(each x ds body)` has no binder bracket and iterates a data
        // structure directly, so a literal there is correct code.
        let tree = SyntaxTree::parse_with_dialect("(each x [1 2 3] (print x))", Dialect::Janet)
            .expect("parse");
        assert!(collect(Dialect::Janet, &tree).is_empty());
    }

    #[test]
    fn the_candidate_count_counts_every_each() {
        let tree = SyntaxTree::parse_with_dialect(
            "(each [k v (pairs t)] (f k)) (each [k v {:a 1}] (f k))",
            Dialect::Fennel,
        )
        .expect("parse");
        assert_eq!(candidate_count(Dialect::Fennel, &tree), 2);
        assert_eq!(collect(Dialect::Fennel, &tree).len(), 1);
    }
}
