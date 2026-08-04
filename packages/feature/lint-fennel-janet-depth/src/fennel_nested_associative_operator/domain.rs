//! `fennel-nested-associative-operator` detection: `(and a (and b c))`, which
//! is `(and a b c)` with an extra level of parentheses.
//!
//! # Primary source
//!
//! `fennel-ls`'s `nested-associative-operator`, enabled by default since 0.2.2
//! (`src/fennel-ls/lint.fnl:909-953`), over the seven heads in its
//! `associative-ops` table (`lint.fnl:35`):
//!
//! ```text
//! (local associative-ops {:+ true :* true :and true :or true
//!                         :band true :bor true :.. true})
//! ```
//!
//! # Why `+` and `*` are not in [`HEADS`]
//!
//! Because they are not associative. Fennel's operators compile to Lua's, Lua's
//! numbers are IEEE-754 doubles, and floating-point addition and multiplication
//! are famously not associative — so the "collapse" this lint advises can
//! change the value. Measured on `fennel 1.6.1`:
//!
//! ```text
//! (* 1e300 (* 1e300 1e-300))  =>  1e+300
//! (* 1e300 1e300 1e-300)      =>  inf
//!
//! (= (+ 1e16 (+ 1 1)) (+ 1e16 1 1))  =>  false
//! ```
//!
//! A finite result becoming `inf` is not a formatting preference. Reporting
//! the shape would be telling the author to introduce an overflow, so `+` and
//! `*` are excluded and this rule keeps only the five operators whose
//! collapse is exact.
//!
//! Those five were checked rather than assumed, also on 1.6.1:
//!
//! ```text
//! (= (and 1 (and false 3)) (and 1 false 3))    => true
//! (= (or false (or nil 3)) (or false nil 3))   => true
//! (= (.. "a" (.. "b" "c")) (.. "a" "b" "c"))   => true
//! (band 12 (band 10 6)) and (band 12 10 6)     => 12 & (10 & 6) / 12 & 10 & 6
//! ```
//!
//! `and` and `or` keep their short-circuit order under collapsing, string
//! concatenation is associative, and `&`/`|` are associative on integers.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::head_symbol;

pub const DIALECTS: [Dialect; 1] = [Dialect::Fennel];

/// The operators whose nesting can be flattened without changing the value.
///
/// `fennel-ls` also lists `+` and `*`; see the module docs for the measured
/// counter-example that keeps them out.
pub const HEADS: [&str; 5] = ["and", "or", "..", "band", "bor"];

/// One operator call containing a nested call to the same operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedOperator {
    /// The inner call, which is the part that can go away.
    pub span: ByteSpan,
    /// The outer call — the node the engine dispatched on, and so the span the
    /// rule's quote guard is asked about. See `fennel_bad_unpack`'s
    /// `form_span` for why the two spans can disagree.
    pub form_span: ByteSpan,
    pub operator: String,
}

/// Examines one form.
///
/// Looks only at the form's own arguments — one level, no descent — so a
/// three-deep nest reports twice, once per level, each naming its own inner
/// call.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<NestedOperator> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let operator = head_symbol(view)?;
    if !HEADS.contains(&operator) {
        return None;
    }
    // `1..` skips the head. The skip states the intent but cannot change the
    // answer, and mutation testing confirms it: widening the range to `0..`
    // kills no test, because reaching this line already required child 0 to be
    // an *atom* (that is how `operator` was obtained) and `head_symbol`
    // answers `None` for every atom. Kept for the reader, recorded here as
    // knowingly unkillable rather than left looking like an untested guard.
    let nested = view
        .children
        .get(1..)?
        .iter()
        .find(|argument| head_symbol(argument) == Some(operator))?;
    Some(NestedOperator {
        span: nested.span,
        form_span: view.span,
        operator: operator.to_owned(),
    })
}

/// Every collapsible nest in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<NestedOperator> {
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

/// Every call to one of these five operators. The denominator.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| HEADS.contains(&head)) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operators(source: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Fennel).expect("parse");
        collect(Dialect::Fennel, &tree)
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    #[test]
    fn every_declared_operator_reports_its_own_nest() {
        for operator in HEADS {
            let source = format!("({operator} a ({operator} b c) d)");
            assert_eq!(operators(&source), vec![operator], "{operator}");
        }
    }

    #[test]
    fn the_two_floating_point_operators_are_excluded() {
        // `(* 1e300 (* 1e300 1e-300))` is 1e300 and `(* 1e300 1e300 1e-300)`
        // is inf, so the collapse this lint would advise is not value
        // preserving. Measured on fennel 1.6.1.
        assert!(operators("(+ a (+ b c))").is_empty());
        assert!(operators("(* a (* b c))").is_empty());
    }

    #[test]
    fn a_different_operator_inside_is_not_a_nest() {
        assert!(operators("(and a (or b c))").is_empty());
        assert!(operators("(or a (and b c))").is_empty());
        assert!(operators("(.. a (tostring b))").is_empty());
    }

    #[test]
    fn the_head_position_is_not_an_argument() {
        // The nested call has to be an *argument*; the operator's own head is
        // child 0 and comparing it to itself would report every single call.
        assert!(operators("(and a b)").is_empty());
        assert!(operators("(or x)").is_empty());
    }

    #[test]
    fn the_finding_points_at_the_inner_call() {
        let source = "(and outer (and inner-a inner-b) tail)";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Fennel).expect("parse");
        let item = collect(Dialect::Fennel, &tree).remove(0);
        assert_eq!(
            &source[item.span.start().get()..item.span.end().get()],
            "(and inner-a inner-b)"
        );
    }

    #[test]
    fn a_nest_in_any_argument_position_is_found_not_just_the_first() {
        assert_eq!(operators("(and a b (and c d))"), vec!["and"]);
        assert_eq!(operators("(.. a b c (.. d e))"), vec![".."]);
    }

    #[test]
    fn a_three_deep_nest_reports_once_per_level() {
        // Two levels of collapsible nesting, two findings.
        assert_eq!(operators("(and a (and b (and c d)))"), vec!["and", "and"]);
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // Clojure and Common Lisp both have `and`/`or`, and the same nesting
        // there is a different language's question.
        for dialect in [Dialect::Janet, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect("(and a (and b c))", dialect).expect("parse");
            assert!(collect(dialect, &tree).is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn the_candidate_count_counts_every_call_to_these_operators() {
        let tree =
            SyntaxTree::parse_with_dialect("(and a (and b c)) (or x y) (+ p q)", Dialect::Fennel)
                .expect("parse");
        // Outer `and`, inner `and`, and the `or`. The `+` is out of scope.
        assert_eq!(candidate_count(Dialect::Fennel, &tree), 3);
        assert_eq!(collect(Dialect::Fennel, &tree).len(), 1);
    }
}
