//! Telling LFE's two unrelated `catch`es apart.

use paredit_core_syntax::dialect::Dialect;

/// LFE only. `catch` is Erlang's catch-all BIF here; in every other dialect in
/// this workspace the symbol means something else entirely (a Common Lisp
/// `catch`/`throw` tag, a Clojure `try` clause), so a wider scope would report
/// unrelated code.
pub const DIALECTS: [Dialect; 1] = [Dialect::Lfe];

pub const HEAD_NAMES: [&str; 1] = ["catch"];

/// The heads whose *direct child* named `catch` is a clause introducer rather
/// than the catch-all expression.
///
/// This is the whole difficulty of the rule. `(catch Expr)` is the old
/// expression form, and `(try Expr (catch Clauses…))` is a `try` clause — and
/// both are a paren list whose head is `catch` with one further child, so the
/// node alone cannot say which. Only the parent can.
const CLAUSE_PARENTS: [&str; 1] = ["try"];

/// Whether a `catch` form directly inside a parent headed by `parent_head` is
/// a clause of that parent rather than the catch-all expression.
#[must_use]
pub fn is_clause_of(parent_head: Option<&str>) -> bool {
    parent_head.is_some_and(|head| CLAUSE_PARENTS.contains(&head))
}
