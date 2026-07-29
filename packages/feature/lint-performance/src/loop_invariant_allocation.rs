//! `loop-invariant-allocation`: building the same object on every round.
//!
//! `(dolist (x xs) (let ((table (make-hash-table))) …))` allocates a fresh
//! table per element. When the constructor's arguments mention none of the
//! loop's variables, every round builds an object indistinguishable from the
//! last, and one allocation before the loop would do.
//!
//! The invariance check is the same one `linear-search-in-loop` uses, and it is
//! deliberately conservative in the direction of silence: if the constructor
//! mentions a loop variable *anywhere*, even in a position that does not affect
//! the result, the rule says nothing.
//!
//! Report-only. Hoisting changes lifetime: a table built once and reused across
//! rounds keeps whatever the previous round put in it, which is sometimes the
//! point and sometimes a bug. That is a decision about the program, not a
//! rewrite.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{for_each_subview, list_head};

use crate::shared::{ITERATION_HEADS, iteration_variables, mentions_any, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "loop-invariant-allocation",
    RuleCategory::Allocation,
    Severity::Warning,
    "an allocation inside a loop whose arguments never mention the loop, so every round builds the same object",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "A constructor whose arguments do not depend on the loop produces an equivalent object \
         every round. Building it once before the loop replaces n allocations with one.",
    )
    .with_example(
        "(dolist (x xs) (member x (list :a :b :c)))",
        "(let ((keys (list :a :b :c))) (dolist (x xs) (member x keys)))",
    )
    .with_caveat(
        "Hoisting changes lifetime — one shared object instead of n fresh ones — so this is a \
         report, not a fix. It is tagged `pedantic` because a short-lived allocation is often \
         cheaper than the reader effort of the hoisted binding.",
    ),
);

const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("loop"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
    NormalizedHead::new("mapc"),
    NormalizedHead::new("maplist"),
];

/// The constructors worth hoisting: each allocates, and none is so cheap that
/// the finding would be noise.
const CONSTRUCTORS: [&str; 8] = [
    "make-hash-table",
    "make-array",
    "make-string",
    "make-sequence",
    "make-list",
    "make-instance",
    "make-pathname",
    "coerce",
];

/// One allocation in a loop body that the loop does not influence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantAllocation {
    pub span: ByteSpan,
    pub constructor: String,
}

/// Every loop-invariant allocation in one iteration form's body.
#[must_use]
pub fn examine_loop(view: &ExpressionView) -> Vec<InvariantAllocation> {
    if !list_head(view).is_some_and(|head| symbol_in(head, &ITERATION_HEADS)) {
        return Vec::new();
    }
    let variables = iteration_variables(view);
    if variables.is_empty() {
        return Vec::new();
    }

    let mut found = Vec::new();
    for_each_subview(view, |subview| {
        let Some(head) = list_head(subview) else {
            return;
        };
        if !symbol_in(head, &CONSTRUCTORS) {
            return;
        }
        // The head is a symbol, so only the arguments can carry a dependency.
        if subview
            .children
            .iter()
            .skip(1)
            .any(|argument| mentions_any(argument, &variables))
        {
            return;
        }
        found.push(InvariantAllocation {
            span: subview.span,
            constructor: unqualified(head).to_ascii_lowercase(),
        });
    });
    found
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
        for allocation in examine_loop(view) {
            sink.report(
                allocation.span,
                format!(
                    "this {} depends on nothing the loop changes, so every round allocates an \
                     equivalent object; consider building it once before the loop",
                    allocation.constructor
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

    fn constructors(input: &str) -> Vec<String> {
        examine_loop(&form(input))
            .into_iter()
            .map(|item| item.constructor)
            .collect()
    }

    #[test]
    fn flags_a_constructor_that_ignores_the_loop() {
        assert_eq!(
            constructors("(dolist (x xs) (let ((h (make-hash-table :test #'equal))) (use x h)))"),
            vec!["make-hash-table"]
        );
    }

    #[test]
    fn does_not_flag_a_constructor_that_uses_a_loop_variable() {
        assert!(constructors("(dotimes (i n) (make-array i))").is_empty());
        assert!(constructors("(dolist (x xs) (make-instance 'thing :value x))").is_empty());
    }

    #[test]
    fn reads_the_dependency_through_a_nested_argument() {
        assert!(constructors("(dolist (x xs) (make-array (length (items x))))").is_empty());
    }

    #[test]
    fn flags_every_constructor_in_the_list() {
        assert_eq!(
            constructors("(dotimes (i 3) (make-list 4))"),
            vec!["make-list"]
        );
        assert_eq!(
            constructors("(dotimes (i 3) (make-string 8 :initial-element #\\a))"),
            vec!["make-string"]
        );
    }

    #[test]
    fn an_allocation_outside_a_loop_reports_nothing() {
        assert!(constructors("(make-hash-table)").is_empty());
    }

    #[test]
    fn a_loop_with_no_readable_bindings_reports_nothing() {
        assert!(constructors("(loop (make-hash-table))").is_empty());
    }

    #[test]
    fn several_invariant_allocations_are_all_reported() {
        assert_eq!(
            constructors("(dolist (x xs) (list (make-hash-table) (make-array 4)))"),
            vec!["make-hash-table", "make-array"]
        );
    }
}
