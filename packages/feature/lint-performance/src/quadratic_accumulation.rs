//! `quadratic-accumulation`: growing a sequence from its own tail, in a loop.
//!
//! `(setf acc (append acc (list x)))` inside an iteration copies everything
//! accumulated so far on every round, so building n elements costs O(n²). The
//! standard rewrite — push onto the front and `nreverse` once at the end — is
//! O(n), and the same shape applies to `nconc`, `concatenate`, and the
//! `append`-family `push`/`setq` spellings.
//!
//! Detection is keyed on the *iteration* form rather than the assignment, and
//! that is the whole design: the finding is not "this call is slow" (it is
//! not — one `append` is fine) but "this call is slow *because it repeats*".
//! Without the enclosing loop there is nothing to report, and a rule keyed on
//! `setf` would have no way to see it.
//!
//! The self-reference check is what keeps it precise: `(setf acc (append xs
//! ys))` inside a loop is not quadratic, because the result does not grow.
//! Only an accumulator that appears in its own new value does.
//!
//! Report-only. The rewrite reverses the order elements are added in and has
//! to place an `nreverse` somewhere the rule cannot see — after the loop, in
//! whatever the enclosing form returns.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

use crate::shared::{ITERATION_HEADS, symbol_in, symbol_is, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "quadratic-accumulation",
    RuleCategory::Performance,
    Severity::Warning,
    "a loop that grows a sequence by appending to its own accumulated value, which is quadratic",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`append` and `nconc` copy or walk their first argument. Passing the accumulator as that \
         argument on every round re-walks everything gathered so far, so building n elements \
         costs O(n²). Pushing onto the front and reversing once at the end costs O(n).",
    )
    .with_example(
        "(dolist (x xs) (setf acc (append acc (list x))))",
        "(dolist (x xs) (push x acc))  ; then (nreverse acc)",
    )
    .with_caveat(
        "An assignment whose new value does not mention the accumulator — `(setf acc (append as \
         bs))` — is not accumulation and is left alone, however deep in a loop it sits.",
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

/// The operators that make an assignment quadratic when the accumulator is
/// their first argument.
const GROWING_OPERATORS: [&str; 5] = ["append", "nconc", "concatenate", "revappend", "append!"];

/// One quadratic accumulation found inside a loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accumulation {
    pub span: ByteSpan,
    /// The variable being grown.
    pub variable: String,
    /// The operator doing the growing.
    pub operator: String,
}

/// Whether `view` mentions the symbol `name` anywhere.
fn mentions(view: &ExpressionView, name: &str) -> bool {
    if let Some(text) = atom_text(view) {
        return symbol_is(text, unqualified(name));
    }
    view.children.iter().any(|child| mentions(child, name))
}

/// Reads one assignment and reports the accumulation it performs, if any.
///
/// `setf`/`setq` with several place-value pairs are read pair by pair, because
/// `(setf a (append a …) b 1)` is exactly as quadratic as the single-pair
/// spelling and writing them together is common.
fn examine_assignment(view: &ExpressionView, found: &mut Vec<Accumulation>) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["setf", "setq"]) {
        return;
    }
    let pairs = view.children[1..].chunks_exact(2);
    for pair in pairs {
        let (place, value) = (&pair[0], &pair[1]);
        // Only a bare variable is an accumulator this rule can reason about; a
        // structured place like `(gethash k h)` has its own cost story.
        let Some(name) = atom_text(place) else {
            continue;
        };
        let Some(operator) = list_head(value) else {
            continue;
        };
        if !symbol_in(operator, &GROWING_OPERATORS) {
            continue;
        }
        // Quadratic exactly when the accumulator is what gets re-walked, which
        // is the *first* sequence argument. `concatenate` puts a result-type
        // argument first, so its sequences start one later.
        let first_sequence = if symbol_is(operator, "concatenate") {
            2
        } else {
            1
        };
        let Some(target) = value.children.get(first_sequence) else {
            continue;
        };
        if !mentions(target, name) {
            continue;
        }
        found.push(Accumulation {
            span: view.span,
            variable: name.to_owned(),
            operator: unqualified(operator).to_ascii_lowercase(),
        });
    }
}

/// Every quadratic accumulation in the body of one iteration form.
#[must_use]
pub fn examine_loop(view: &ExpressionView) -> Vec<Accumulation> {
    if !list_head(view).is_some_and(|head| symbol_in(head, &ITERATION_HEADS)) {
        return Vec::new();
    }
    let mut found = Vec::new();
    for_each_subview(view, |subview| examine_assignment(subview, &mut found));
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
        for accumulation in examine_loop(view) {
            sink.report(
                accumulation.span,
                format!(
                    "{} grows {} from its own value inside a loop, which costs O(n²); \
                     push onto the front and reverse once after the loop",
                    accumulation.operator, accumulation.variable
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

    fn variables(input: &str) -> Vec<String> {
        examine_loop(&form(input))
            .into_iter()
            .map(|item| item.variable)
            .collect()
    }

    #[test]
    fn flags_the_classic_dolist_append() {
        assert_eq!(
            variables("(dolist (x xs) (setf acc (append acc (list x))))"),
            vec!["acc"]
        );
    }

    #[test]
    fn flags_every_growing_operator() {
        assert_eq!(
            variables("(dotimes (i 10) (setq acc (nconc acc (list i))))"),
            vec!["acc"]
        );
        assert_eq!(
            variables(r#"(loop for x in xs do (setf s (concatenate 'string s "-")))"#),
            vec!["s"]
        );
    }

    #[test]
    fn does_not_flag_an_assignment_that_does_not_grow_itself() {
        assert!(variables("(dolist (x xs) (setf acc (append as bs)))").is_empty());
    }

    #[test]
    fn does_not_flag_the_accumulator_appearing_only_as_a_later_argument() {
        // `(append (list x) acc)` prepends: it copies the one-element list, not
        // the accumulator, and is O(1) per round.
        assert!(variables("(dolist (x xs) (setf acc (append (list x) acc)))").is_empty());
    }

    #[test]
    fn does_not_flag_an_append_outside_any_loop() {
        assert!(variables("(setf acc (append acc (list x)))").is_empty());
    }

    #[test]
    fn reads_the_accumulator_through_a_nested_expression() {
        assert_eq!(
            variables("(dolist (x xs) (setf acc (append (reverse acc) (list x))))"),
            vec!["acc"]
        );
    }

    #[test]
    fn reads_every_pair_of_a_multi_place_setf() {
        assert_eq!(
            variables("(dolist (x xs) (setf n 1 acc (append acc (list x))))"),
            vec!["acc"]
        );
    }

    #[test]
    fn ignores_a_structured_place() {
        // `(setf (gethash k h) (append (gethash k h) …))` is a different cost
        // story and needs the hash lookup reasoned about, not the variable.
        assert!(
            variables("(dolist (x xs) (setf (gethash k h) (append (gethash k h) (list x))))")
                .is_empty()
        );
    }

    #[test]
    fn finds_an_accumulation_nested_deep_in_the_body() {
        assert_eq!(
            variables(
                "(dolist (x xs) (when (good x) (let ((y x)) (setf acc (append acc (list y))))))"
            ),
            vec!["acc"]
        );
    }

    #[test]
    fn a_form_that_is_not_an_iteration_reports_nothing() {
        assert!(variables("(let ((acc nil)) (setf acc (append acc (list x))))").is_empty());
    }
}
