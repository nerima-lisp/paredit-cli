//! `length-emptiness-test`: asking `length` whether a sequence is empty.
//!
//! `(= (length xs) 0)` walks the entire sequence to learn something the first
//! cons cell already answers. On a list that is O(n) where `null` is O(1); on a
//! circular list it does not terminate at all.
//!
//! The rule reads the comparison, not the sequence, so it fires on the whole
//! family: `=`, `/=`, `<`, `>`, `<=`, `>=`, `zerop`, `plusp`, and `eql`
//! against a literal `0` — each of which is some project's preferred spelling
//! of the same mistake.
//!
//! Report-only, deliberately. The rewrite depends on what the sequence *is*:
//! `(null xs)` is right for a list and wrong for a vector, and nothing at this
//! layer can tell them apart. Suggesting `null` in the message and refusing to
//! apply it is the honest position.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::view_query::atom_text;

use crate::shared::{calls, symbol_is};

pub const META: RuleMeta = RuleMeta::new(
    "length-emptiness-test",
    RuleCategory::Performance,
    Severity::Warning,
    "a (length …) call used only to decide whether a sequence is empty, which walks the whole sequence",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`length` traverses the whole sequence. Comparing its result against 0 asks an O(n) \
         question whose answer the first element already gives, and never terminates on a \
         circular list.",
    )
    .with_example("(when (= (length items) 0) (return))", "(when (null items) (return))")
    .with_caveat(
        "Comparisons against any other constant are left alone: `(> (length xs) 3)` genuinely \
         needs four elements counted, and no shorter form expresses it.",
    ),
);

/// The comparisons that can express emptiness against a literal `0`, plus the
/// two predicates that carry the constant implicitly.
const HEADS: [NormalizedHead; 9] = [
    NormalizedHead::new("="),
    NormalizedHead::new("/="),
    NormalizedHead::new("<"),
    NormalizedHead::new(">"),
    NormalizedHead::new("<="),
    NormalizedHead::new(">="),
    NormalizedHead::new("eql"),
    NormalizedHead::new("zerop"),
    NormalizedHead::new("plusp"),
];

/// What the comparison is really asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptinessTest {
    /// True exactly when the sequence is empty.
    Empty,
    /// True exactly when the sequence has at least one element.
    NonEmpty,
}

impl EmptinessTest {
    const fn suggestion(self) -> &'static str {
        match self {
            Self::Empty => "(null …)",
            Self::NonEmpty => "(consp …)",
        }
    }
}

/// Whether `view` is a literal integer zero.
fn is_zero(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view).is_some_and(|text| text == "0")
}

/// Whether `view` is a `(length …)` call with exactly one argument.
fn is_length_call(view: &ExpressionView) -> bool {
    calls(view, &["length"]) && view.children.len() == 2
}

/// Reads one comparison and reports which emptiness question it asks, or
/// `None` if it asks something else.
///
/// Both operand orders are read, and the answer flips with them: `(< 0 (length
/// xs))` and `(> (length xs) 0)` are the same question spelled two ways, and a
/// rule that only knew one of them would leave half the occurrences in a
/// codebase unreported.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<EmptinessTest> {
    let head = paredit_core_syntax::view_query::list_head(view)?;

    // The two predicates carry their constant in their name.
    if symbol_is(head, "zerop") || symbol_is(head, "plusp") {
        if view.children.len() != 2 || !is_length_call(&view.children[1]) {
            return None;
        }
        return Some(if symbol_is(head, "zerop") {
            EmptinessTest::Empty
        } else {
            EmptinessTest::NonEmpty
        });
    }

    // Everything else is a binary comparison with a literal 0 on one side.
    if view.children.len() != 3 {
        return None;
    }
    let (left, right) = (&view.children[1], &view.children[2]);
    let length_first = is_length_call(left) && is_zero(right);
    let zero_first = is_zero(left) && is_length_call(right);
    if !length_first && !zero_first {
        return None;
    }

    // `length` is a non-negative integer, so `< 0`, `<= -` and `>= 0` are not
    // emptiness questions: the first is never true and the last is always true.
    // Those are somebody else's finding, not this rule's.
    Some(match (head, length_first) {
        (h, _) if symbol_is(h, "=") || symbol_is(h, "eql") => EmptinessTest::Empty,
        (h, _) if symbol_is(h, "/=") => EmptinessTest::NonEmpty,
        (h, true) if symbol_is(h, ">") => EmptinessTest::NonEmpty,
        (h, true) if symbol_is(h, "<=") => EmptinessTest::Empty,
        (h, false) if symbol_is(h, "<") => EmptinessTest::NonEmpty,
        (h, false) if symbol_is(h, ">=") => EmptinessTest::Empty,
        _ => return None,
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(test) = examine(view) {
            sink.report(
                view.span,
                format!(
                    "this asks (length …) an emptiness question, which walks the whole sequence; \
                     {} answers it in constant time for a list",
                    test.suggestion()
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn form(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn test_of(input: &str) -> Option<EmptinessTest> {
        examine(&form(input))
    }

    #[test]
    fn equality_against_zero_asks_emptiness() {
        assert_eq!(test_of("(= (length xs) 0)"), Some(EmptinessTest::Empty));
        assert_eq!(test_of("(= 0 (length xs))"), Some(EmptinessTest::Empty));
        assert_eq!(test_of("(eql (length xs) 0)"), Some(EmptinessTest::Empty));
    }

    #[test]
    fn inequality_against_zero_asks_non_emptiness() {
        assert_eq!(test_of("(/= (length xs) 0)"), Some(EmptinessTest::NonEmpty));
        assert_eq!(test_of("(> (length xs) 0)"), Some(EmptinessTest::NonEmpty));
        assert_eq!(test_of("(< 0 (length xs))"), Some(EmptinessTest::NonEmpty));
    }

    #[test]
    fn ordered_comparisons_read_the_same_question_from_either_side() {
        assert_eq!(test_of("(<= (length xs) 0)"), Some(EmptinessTest::Empty));
        assert_eq!(test_of("(>= 0 (length xs))"), Some(EmptinessTest::Empty));
    }

    #[test]
    fn the_predicates_carry_their_constant_in_their_name() {
        assert_eq!(test_of("(zerop (length xs))"), Some(EmptinessTest::Empty));
        assert_eq!(
            test_of("(plusp (length xs))"),
            Some(EmptinessTest::NonEmpty)
        );
    }

    #[test]
    fn a_comparison_that_is_never_or_always_true_is_not_this_rules_finding() {
        // `length` is non-negative, so these say nothing about emptiness.
        assert_eq!(test_of("(< (length xs) 0)"), None);
        assert_eq!(test_of("(>= (length xs) 0)"), None);
    }

    #[test]
    fn a_comparison_against_another_constant_is_left_alone() {
        assert_eq!(test_of("(= (length xs) 1)"), None);
        assert_eq!(test_of("(> (length xs) 3)"), None);
    }

    #[test]
    fn a_call_that_is_not_length_is_left_alone() {
        assert_eq!(test_of("(= (count x xs) 0)"), None);
        assert_eq!(test_of("(zerop (hash-table-count h))"), None);
    }

    #[test]
    fn the_package_qualified_spelling_is_read_the_same() {
        assert_eq!(
            test_of("(cl:= (cl:length xs) 0)"),
            Some(EmptinessTest::Empty)
        );
    }

    #[test]
    fn a_length_call_with_a_start_argument_is_not_the_one_argument_form() {
        // `(length xs 0)` is not legal CL, but a malformed call is
        // `accessor-arity`'s finding, not a cost claim this rule should make.
        assert_eq!(test_of("(= (length xs 0) 0)"), None);
    }
}
