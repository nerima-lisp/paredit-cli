//! `fennel-bad-unpack` detection: an `unpack` call in the last argument
//! position of a Fennel operator, which silently drops every value but the
//! first.
//!
//! # Primary source
//!
//! `fennel-ls`, the language server Fennel's own `src/linter.fnl` names as
//! "a real linter", ships this as `bad-unpack`, enabled by default since
//! 0.1.0 (`src/fennel-ls/lint.fnl:352-400`):
//!
//! ```text
//! (if (and (op? op)
//!          (list? last)
//!          (or (sym? (. last 1) :unpack)
//!              (sym? (. last 1) :_G.unpack)
//!              (sym? (. last 1) :table.unpack)))
//!     {:ast last :message "faulty unpack call: … isn't variadic at runtime."})
//! ```
//!
//! # Why it is a real defect
//!
//! Fennel's operators are not function calls. They compile to Lua's binary
//! operators, and Lua truncates a multiple-value expression to one value in
//! every position except the last of an argument list. So the extra values
//! `unpack` returns are discarded with no error and no warning.
//!
//! Verified against `fennel 1.6.1` by reading the generated Lua rather than by
//! reasoning about it:
//!
//! ```text
//! $ fennel --compile -    <<< '(print (+ 1 (table.unpack [2 3 4])))'
//! return print((1 + table.unpack({2, 3, 4})))
//! $ fennel -e '(print (+ 1 (table.unpack [2 3 4])))'
//! 3
//! ```
//!
//! `3`, not `10`: `1 + 2`, and `3` and `4` are gone.
//!
//! # Where this rule departs from `fennel-ls`, and why
//!
//! `fennel-ls` fires whenever the operator's last argument is an `unpack`
//! call. That is wrong for a **one-argument** call to five of the operators,
//! because Fennel compiles those away entirely and the multiple values pass
//! straight through. Again read off the generated Lua from `fennel 1.6.1`:
//!
//! ```text
//! (.. (table.unpack ["a" "b" "c"]))   =>  table.unpack({"a", "b", "c"})
//! (and (table.unpack [1 2]))          =>  table.unpack({1, 2})
//! (or  (table.unpack [1 2]))          =>  table.unpack({1, 2})
//! (%   (table.unpack [1 2]))          =>  table.unpack({1, 2})
//! (^   (table.unpack [1 2]))          =>  table.unpack({1, 2})
//! ```
//!
//! and confirmed by running it: `(select "#" (.. (table.unpack ["a" "b" "c"])))`
//! is `3`, so nothing was dropped. `fennel-ls`'s own documentation example for
//! this lint — ``(.. (unpack ["a" "b" "c"]))  ; Only concatenates "a"`` — is
//! therefore incorrect as written for Fennel 1.6.1. `PASSTHROUGH_UNARY` is
//! that measured exception list.
//!
//! Every other operator does truncate even at one argument, because it
//! compiles to a binary operation against an identity element:
//!
//! ```text
//! (+ (table.unpack [1 2]))     =>  (0 + table.unpack({1, 2}))
//! (* (table.unpack [1 2]))     =>  (1 * table.unpack({1, 2}))
//! (band (table.unpack [1 2]))  =>  (-1 & table.unpack({1, 2}))
//! ```
//!
//! so those stay in scope at one argument.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::{head_symbol, symbol_text};

pub const DIALECTS: [Dialect; 1] = [Dialect::Fennel];

/// Fennel's operator set, transcribed from `fennel-ls`'s `ops` table
/// (`src/fennel-ls/lint.fnl:25-27`).
///
/// These are exactly the heads that compile to a Lua operator rather than to a
/// function call, which is what makes them non-variadic at runtime.
pub const HEADS: [&str; 23] = [
    "+", "-", "*", "/", "//", "%", "^", ">", "<", ">=", "<=", "=", "not=", "..", ".", "and", "or",
    "band", "bor", "bxor", "bnot", "lshift", "rshift",
];

/// The operators whose **one-argument** form Fennel compiles away, so that
/// multiple values survive it.
///
/// Measured, not assumed — see the module docs for the generated Lua. A
/// one-argument call to any of these is not a defect and is not reported.
const PASSTHROUGH_UNARY: [&str; 5] = ["..", "and", "or", "%", "^"];

/// The three spellings of Lua's `unpack` that `fennel-ls` recognises
/// (`lint.fnl:381-384`).
///
/// `unpack` is the Lua 5.1 global; `table.unpack` is its 5.2+ home; `_G.unpack`
/// is how one reaches the global explicitly from a scope that shadows it.
const UNPACK_HEADS: [&str; 3] = ["unpack", "_G.unpack", "table.unpack"];

/// One operator call whose trailing `unpack` will be truncated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadUnpack {
    /// The `unpack` call itself, which is the argument that will not do what
    /// it looks like it does.
    pub span: ByteSpan,
    /// The whole operator call — the node the engine dispatched on, and so the
    /// span the rule's quote guard is asked about. Reported so that an audit
    /// outside the engine can apply the identical guard; asking about
    /// [`Self::span`] instead gives a different answer, because the `,` of
    /// `` `(and ,x ,(unpack ys)) `` escapes the quasiquote at the inner node
    /// and not at the outer one.
    pub form_span: ByteSpan,
    /// The operator, so the message can name it.
    pub operator: String,
    /// The `unpack` spelling, so the message can name that too.
    pub unpack: String,
}

/// Whether `view` is a call to one of the three `unpack` spellings.
fn is_unpack_call(view: &ExpressionView) -> Option<&str> {
    let head = head_symbol(view)?;
    UNPACK_HEADS.contains(&head).then_some(head)
}

/// Examines one form.
///
/// Cheap and allocation-free until a finding exists: two slice `contains`
/// calls and one child lookup.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<BadUnpack> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let operator = head_symbol(view)?;
    if !HEADS.contains(&operator) {
        return None;
    }
    // Head plus arguments.
    let arguments = view.children.len().checked_sub(1)?;
    if arguments == 1 && PASSTHROUGH_UNARY.contains(&operator) {
        return None;
    }
    // No explicit `arguments == 0` guard. Mutation testing showed one to be
    // dead: on `(+)` the "last child" is the head atom itself, and
    // `is_unpack_call` asks `head_symbol`, which answers `None` for anything
    // that is not a paren list. `an_empty_operator_call_is_not_indexed_out_of_bounds`
    // pins that behaviour through this path rather than through a guard that
    // no test could ever kill.
    let last = view.children.last()?;
    let unpack = is_unpack_call(last)?;
    Some(BadUnpack {
        span: last.span,
        form_span: view.span,
        operator: operator.to_owned(),
        unpack: unpack.to_owned(),
    })
}

/// The advice this operator earns, which differs for `..`.
///
/// `fennel-ls` offers `table.concat` for `..` specifically, because that is the
/// variadic function the author almost certainly wanted; for the arithmetic and
/// bitwise operators there is no such one-liner and a fold is the answer.
#[must_use]
pub fn advice_for(operator: &str) -> &'static str {
    if operator == ".." {
        "use (table.concat …) instead"
    } else {
        "use accumulate/a loop when the argument count is dynamic"
    }
}

/// Every truncating `unpack` in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<BadUnpack> {
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

/// Every operator call in the file whose last argument is a `(…)` list — the
/// population this rule chooses from. The denominator.
///
/// Not "every operator call": an operator whose last argument is an atom could
/// never have been reported, so counting those would inflate the denominator
/// and make a zero-finding sweep look more informative than it is.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        let is_operator_call = head_symbol(view).is_some_and(|head| HEADS.contains(&head));
        if is_operator_call
            && view.children.len() > 1
            && view
                .children
                .last()
                .is_some_and(|last| symbol_text(last).is_none() && head_symbol(last).is_some())
        {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source: &str) -> Vec<BadUnpack> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Fennel).expect("parse");
        collect(Dialect::Fennel, &tree)
    }

    fn operators(source: &str) -> Vec<String> {
        found(source)
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    #[test]
    fn the_measured_arithmetic_case_is_reported() {
        // `fennel -e '(print (+ 1 (table.unpack [2 3 4])))'` prints 3, not 10.
        assert_eq!(operators("(+ 1 (table.unpack [2 3 4]))"), vec!["+"]);
    }

    #[test]
    fn all_three_unpack_spellings_are_recognised() {
        for spelling in ["unpack", "_G.unpack", "table.unpack"] {
            let source = format!("(.. \"x\" ({spelling} xs))");
            let items = found(&source);
            assert_eq!(items.len(), 1, "{spelling} was not recognised");
            assert_eq!(items[0].unpack, spelling);
        }
    }

    #[test]
    fn the_finding_points_at_the_unpack_call_not_the_operator() {
        let source = "(+ 1 (table.unpack [2 3]))";
        let item = found(source).remove(0);
        let text = &source[item.span.start().get()..item.span.end().get()];
        assert_eq!(text, "(table.unpack [2 3])");
    }

    #[test]
    fn only_the_last_argument_counts() {
        // `(+ (table.unpack xs) 1)` compiles to `(table.unpack(xs) + 1)`, where
        // the truncation to one value is Lua's ordinary behaviour for a
        // non-final expression and is what the author asked for.
        assert!(operators("(+ (table.unpack xs) 1)").is_empty());
    }

    #[test]
    fn the_five_passthrough_unary_operators_are_exempt_at_one_argument() {
        // Measured: each of these compiles to a bare `table.unpack(...)`.
        for operator in ["..", "and", "or", "%", "^"] {
            assert!(
                operators(&format!("({operator} (table.unpack xs))")).is_empty(),
                "unary {operator} was reported but passes values through"
            );
        }
    }

    #[test]
    fn the_same_five_operators_are_reported_at_two_arguments() {
        // The control for the test above: the exemption is the arity, not the
        // operator. `(.. "x" (table.unpack ["a" "b"]))` really is "xa".
        for operator in ["..", "and", "or", "%", "^"] {
            assert_eq!(
                operators(&format!("({operator} y (table.unpack xs))")),
                vec![operator],
                "two-argument {operator} was not reported"
            );
        }
    }

    #[test]
    fn a_truncating_unary_operator_is_still_reported() {
        // Measured: `(+ (table.unpack [1 2]))` is `(0 + table.unpack({1, 2}))`.
        for operator in [
            "+", "*", "-", "/", "//", "band", "bor", "bxor", "lshift", "rshift",
        ] {
            assert_eq!(
                operators(&format!("({operator} (table.unpack xs))")),
                vec![operator],
                "unary {operator} truncates and should be reported"
            );
        }
    }

    #[test]
    fn an_ordinary_function_call_is_variadic_and_is_left_alone() {
        // This is the whole point of the rule: `print` is a real function, so
        // it receives every value.
        assert!(operators("(print (table.unpack xs))").is_empty());
        assert!(operators("(table.insert t (table.unpack xs))").is_empty());
    }

    #[test]
    fn an_operator_with_no_unpack_is_left_alone() {
        assert!(operators("(+ 1 2 3)").is_empty());
        assert!(operators("(.. \"a\" (tostring x))").is_empty());
    }

    #[test]
    fn an_empty_operator_call_is_not_indexed_out_of_bounds() {
        assert!(operators("(+)").is_empty());
        assert!(operators("(..)").is_empty());
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // In Janet `(+ 1 (table.unpack xs))` is an ordinary variadic call to a
        // real function `+`, and `unpack` is spelled `splice`/`;`.
        for dialect in [Dialect::Janet, Dialect::CommonLisp, Dialect::Clojure] {
            let tree =
                SyntaxTree::parse_with_dialect("(+ 1 (table.unpack xs))", dialect).expect("parse");
            assert!(collect(dialect, &tree).is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn the_candidate_count_counts_operator_calls_that_could_have_been_reported() {
        let tree = SyntaxTree::parse_with_dialect(
            "(+ 1 (table.unpack xs)) (+ 1 (foo)) (+ 1 2) (print (bar))",
            Dialect::Fennel,
        )
        .expect("parse");
        // Two operator calls end in a list; the third ends in an atom and the
        // fourth is not an operator.
        assert_eq!(candidate_count(Dialect::Fennel, &tree), 2);
        assert_eq!(collect(Dialect::Fennel, &tree).len(), 1);
    }
}
