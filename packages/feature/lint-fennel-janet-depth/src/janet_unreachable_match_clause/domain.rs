//! `janet-unreachable-match-clause` detection: a `match` clause written after a
//! catch-all pattern, which can never be reached.
//!
//! # Primary source
//!
//! Janet's own docstring for `match`, read off `janet 1.41.2`:
//!
//! ```text
//! (match x & cases)
//!
//! * symbol -- a pattern that is a symbol will match anything, binding `x`'s
//!   value to that symbol.
//! * `_` symbol -- the last special case is the `_` symbol, which is a
//!   wildcard that will match any value without creating a binding.
//! …
//! Quoting a pattern with `'` will also treat the value as a literal value to
//! match against.
//! ```
//!
//! "will match anything" is the whole rule: once a bare symbol appears in a
//! pattern position, no later clause is reachable. Confirmed by running it:
//!
//! ```janet
//! (match 99 x :first 99 :second)  # => :first
//! (match 99 _ :first 99 :second)  # => :first
//! ```
//!
//! `99` really is the subject, and the clause that matches it literally is
//! still never taken.
//!
//! # The three shapes that are not catch-alls
//!
//! - A **quoted** symbol, `'foo`, is a literal to compare against — the
//!   docstring says so, and `(match 'bar 'foo :matched _ :fell-through)`
//!   answers `:fell-through`.
//! - A **tuple** pattern, `(x (> x 5))`, binds `x` and then requires every
//!   following element to be true, so it is a guard and not a catch-all. The
//!   `(@ sym)` pin is a tuple too.
//! - The **last** element of an odd-length clause list is `match`'s fallback
//!   *expression*, not a pattern. `(match 42 :a :got-a :default-expr)` answers
//!   `:default-expr`, and a symbol there is ordinary code.
//!
//! # Why this is not `janet-dead-branch-on-constant-condition`
//!
//! That rule folds a literal test; this one is about pattern reachability and
//! never looks at the subject at all. Janet's compiler reports the first and
//! not the second — `match` expands to nested `if`s over a gensym, so nothing
//! is constant-folded and no lint fires. This rule is therefore *not* a
//! transcription of an existing check but a consequence of documented,
//! executed semantics.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SyntaxTree};

use crate::support::{head_symbol, symbol_text};

pub const DIALECTS: [Dialect; 1] = [Dialect::Janet];

/// Only `match`.
///
/// `case` compares with `=` and its patterns are values, so an unreachable
/// `case` clause is a duplicate-constant question rather than a catch-all one;
/// `cond` takes expressions, where a literal test is
/// `janet-dead-branch-on-constant-condition`'s business. Neither belongs here.
pub const HEADS: [&str; 1] = ["match"];

/// One `match` clause that can never be reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreachableClause {
    /// The first unreachable pattern.
    pub span: ByteSpan,
    /// The whole `match` — the node the engine dispatched on, and so the span
    /// the rule's quote guard is asked about. See `fennel_bad_unpack`'s
    /// `form_span` for why the two spans can disagree.
    pub form_span: ByteSpan,
    /// The catch-all that shadows it, so the message can name it.
    pub catch_all: String,
    /// How many clauses, including this one, are dead.
    pub shadowed: usize,
}

/// Whether a pattern node matches every possible subject.
///
/// True for a bare, unquoted symbol and for nothing else. An atom carrying any
/// reader prefix is excluded outright: `'foo` is a literal by the docstring,
/// and `;foo` or `,foo` in a pattern position is not something this rule
/// should have an opinion about.
fn is_catch_all(view: &ExpressionView) -> bool {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return false;
    }
    let Some(text) = symbol_text(view) else {
        return false;
    };
    if text.is_empty() {
        return false;
    }
    // A keyword, string, long string, number or buffer is a value pattern.
    let first = text.as_bytes()[0];
    if matches!(first, b':' | b'"' | b'`' | b'@') {
        return false;
    }
    if first.is_ascii_digit() {
        return false;
    }
    if matches!(first, b'-' | b'+' | b'.') && text.len() > 1 && text.as_bytes()[1].is_ascii_digit()
    {
        return false;
    }
    // `true`, `false` and `nil` are values, not binding symbols.
    !matches!(text, "true" | "false" | "nil")
}

/// Examines one form.
///
/// The scan is over the form's own children only — no descent, no root view —
/// so it is O(clauses).
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<UnreachableClause> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let head = head_symbol(view)?;
    if !HEADS.contains(&head) {
        return None;
    }
    if view.delimiter != Some(Delimiter::Paren) {
        return None;
    }
    // children = [match, subject, pattern, body, pattern, body, …, fallback?]
    let clauses = view.children.get(2..)?;
    // An odd tail means the last element is the fallback expression, not a
    // pattern, so it is not part of the pattern/body pairing.
    let paired = clauses.len() - clauses.len() % 2;
    if paired < 4 {
        // Fewer than two clauses: nothing can be shadowed.
        return None;
    }
    // Every pattern except the last pair's: a catch-all in the final pattern
    // position shadows only the fallback expression, which is exactly what a
    // fallback is for.
    let mut index = 0;
    while index + 2 < paired {
        let pattern = clauses.get(index)?;
        if is_catch_all(pattern) {
            let dead = clauses.get(index + 2)?;
            // Remaining pattern/body pairs after the catch-all, plus a
            // fallback if the tail was odd.
            let remaining_pairs = (paired - index - 2) / 2;
            return Some(UnreachableClause {
                span: dead.span,
                form_span: view.span,
                catch_all: symbol_text(pattern)?.to_owned(),
                shadowed: remaining_pairs,
            });
        }
        index += 2;
    }
    None
}

/// Every shadowed `match` clause in one file, at most one per `match`.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<UnreachableClause> {
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

/// Every `match` with at least two clauses. The denominator.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        let is_match = head_symbol(view).is_some_and(|head| HEADS.contains(&head));
        if is_match && view.children.len() >= 6 {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source: &str) -> Vec<UnreachableClause> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Janet).expect("parse");
        collect(Dialect::Janet, &tree)
    }

    fn catch_alls(source: &str) -> Vec<String> {
        found(source)
            .into_iter()
            .map(|item| item.catch_all)
            .collect()
    }

    #[test]
    fn a_binding_symbol_shadows_every_later_clause() {
        // Executed on janet 1.41.2: this answers :first.
        assert_eq!(catch_alls("(match 99 x :first 99 :second)"), vec!["x"]);
    }

    #[test]
    fn the_underscore_wildcard_shadows_too() {
        assert_eq!(catch_alls("(match 99 _ :first 99 :second)"), vec!["_"]);
    }

    #[test]
    fn a_catch_all_in_the_final_pattern_position_is_correct_code() {
        // This is the idiomatic default clause and must never be reported.
        assert!(catch_alls("(match x :a :got-a _ :otherwise)").is_empty());
        assert!(catch_alls("(match x :a 1 :b 2 _ 3)").is_empty());
    }

    /// The odd-tail case, which is the only one the loop bound can get wrong.
    ///
    /// With an even clause list, widening `while index + 2 < paired` to
    /// `while index < paired` changes nothing: `clauses.get(index + 2)` runs
    /// off the end and answers `None`. With an *odd* list the extra slot is
    /// `match`'s fallback expression, and the widened loop reports it as a
    /// shadowed clause. Mutation testing found the bound unkilled until this
    /// test existed.
    ///
    /// Executed on janet 1.41.2: `(match 7 :a 1 _ 2 :fallback)` is `2`, so the
    /// fallback really is unreachable — but a fallback sitting behind a
    /// default clause is a redundant *default*, not a clause someone meant to
    /// be reachable, and this rule deliberately says nothing about it.
    #[test]
    fn a_fallback_expression_behind_a_final_catch_all_is_not_reported() {
        assert!(catch_alls("(match x :a 1 _ 2 :fallback)").is_empty());
    }

    #[test]
    fn a_quoted_symbol_is_a_literal_and_not_a_catch_all() {
        // Executed: `(match 'bar 'foo :matched _ :fell-through)` is
        // :fell-through, so 'foo matched nothing.
        assert!(catch_alls("(match x 'foo :matched-foo 'bar :matched-bar)").is_empty());
    }

    #[test]
    fn a_tuple_pattern_is_a_guard_and_not_a_catch_all() {
        // Executed: `(match 99 (x (> x 5)) :guarded 99 :second)` is :guarded
        // because 99 > 5, but the guard can fail, so the later clause lives.
        assert!(catch_alls("(match v (x (> x 5)) :big 99 :exactly-99)").is_empty());
        // The `(@ sym)` pin is a tuple too.
        assert!(catch_alls("(match v (@ expected) :same :other :no)").is_empty());
    }

    #[test]
    fn value_patterns_are_not_catch_alls() {
        for pattern in [":kw", "\"str\"", "42", "-1", "true", "false", "nil", "@[]"] {
            let source = format!("(match v {pattern} :first :other :second)");
            assert!(
                catch_alls(&source).is_empty(),
                "{pattern} was treated as a catch-all"
            );
        }
    }

    #[test]
    fn an_array_or_struct_pattern_is_not_a_catch_all() {
        assert!(catch_alls("(match v [a b] :pair :other :second)").is_empty());
        assert!(catch_alls("(match v {:k a} :struct :other :second)").is_empty());
    }

    #[test]
    fn the_finding_points_at_the_first_unreachable_pattern() {
        let source = "(match v x :first :dead-pattern :second)";
        let item = found(source).remove(0);
        assert_eq!(
            &source[item.span.start().get()..item.span.end().get()],
            ":dead-pattern"
        );
    }

    #[test]
    fn the_shadowed_count_counts_the_pairs_it_kills() {
        let item = found("(match v x :first :b 2 :c 3 :d 4)").remove(0);
        assert_eq!(item.shadowed, 3);
    }

    #[test]
    fn a_fallback_expression_after_a_catch_all_still_counts_as_shadowed() {
        // Odd tail: `:fallback` is an expression, and the `:b 2` pair between
        // it and the catch-all is dead.
        let item = found("(match v x :first :b 2 :fallback)").remove(0);
        assert_eq!(item.shadowed, 1);
    }

    #[test]
    fn a_single_clause_match_has_nothing_to_shadow() {
        assert!(catch_alls("(match v x :only)").is_empty());
        assert!(catch_alls("(match v x :only :fallback)").is_empty());
        assert!(catch_alls("(match v)").is_empty());
        assert!(catch_alls("(match)").is_empty());
    }

    #[test]
    fn at_most_one_finding_per_match_form() {
        // Two catch-alls in a row would otherwise report twice for one defect.
        assert_eq!(found("(match v x :a y :b z :c)").len(), 1);
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // Fennel's `match` binds symbols too, but its clause grammar and its
        // `_` differ enough that this rule does not claim it.
        for dialect in [Dialect::Fennel, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect("(match 99 x :first 99 :second)", dialect)
                .expect("parse");
            assert!(collect(dialect, &tree).is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn the_candidate_count_counts_multi_clause_matches() {
        let tree = SyntaxTree::parse_with_dialect(
            "(match v x :a 1 :b) (match v :a 1 :b 2) (match v x :only)",
            Dialect::Janet,
        )
        .expect("parse");
        assert_eq!(candidate_count(Dialect::Janet, &tree), 2);
        assert_eq!(collect(Dialect::Janet, &tree).len(), 1);
    }
}
