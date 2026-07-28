//! `read-without-read-eval-guard`: reading data with `#.` still armed.
//!
//! `*read-eval*` defaults to true, and `#.(delete-file "…")` in the input is
//! then executed by `read` itself — before the program has looked at a single
//! form of what it read. Binding `*read-eval*` to `nil` around the call is the
//! one-line remedy, and its absence is the whole finding.
//!
//! The rule looks for the binding as an *enclosing* form, which is why it keys
//! on `let`/`let*`/`handler-case`-style wrappers rather than on `read`: a
//! per-node visit of a `read` call cannot see what encloses it. Keying on the
//! definition and walking down is what makes "is there a guard above this?"
//! answerable at all.
//!
//! Report-only. Adding the binding changes what the surrounding code may do
//! with `#.`, which is a decision about the input format.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, symbol_is};

pub const META: RuleMeta = RuleMeta::new(
    "read-without-read-eval-guard",
    RuleCategory::Security,
    Severity::Warning,
    "a read of external data with no enclosing *read-eval* nil binding, so #. in the input executes",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`*read-eval*` is true by default, so `read` executes any `#.(…)` in its input as part of \
         reading it. Data from outside the program therefore becomes code before the program has \
         inspected one form of it.",
    )
    .with_example(
        "(read stream)",
        "(let ((*read-eval* nil)) (read stream))",
    )
    .with_caveat(
        "Only the definition-level absence is reported. A `read` under a `(let ((*read-eval* \
         nil)) …)` anywhere above it is guarded, however many forms deep it sits.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("lambda"),
];

/// The readers that honour `*read-eval*`.
const READERS: [&str; 4] = [
    "read",
    "read-from-string",
    "read-preserving-whitespace",
    "read-delimited-list",
];

/// One unguarded read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnguardedRead {
    pub span: ByteSpan,
    pub operator: String,
}

/// Whether `view` binds `*read-eval*` to nil.
///
/// Only the `nil` value counts: `(let ((*read-eval* t)) …)` is a binding and is
/// the opposite of a guard.
fn binds_read_eval_nil(view: &ExpressionView) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    if !symbol_in(head, &["let", "let*"]) {
        return false;
    }
    let Some(bindings) = view.children.get(1) else {
        return false;
    };
    bindings.children.iter().any(|binding| {
        let Some(name) = binding.children.first().and_then(atom_text) else {
            return false;
        };
        if !symbol_is(name, "*read-eval*") {
            return false;
        }
        binding
            .children
            .get(1)
            .and_then(atom_text)
            .is_some_and(|value| value.eq_ignore_ascii_case("nil"))
    })
}

/// Every unguarded read in one definition, descending but stopping at any form
/// that establishes the guard.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<UnguardedRead> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["defun", "defmethod", "lambda"]) {
        return Vec::new();
    }
    let mut found = Vec::new();
    walk(view, &mut found);
    found
}

fn walk(view: &ExpressionView, found: &mut Vec<UnguardedRead>) {
    // A guarded subtree is guarded all the way down, so it is not descended
    // into at all — which is both the correct answer and the cheap one.
    if binds_read_eval_nil(view) {
        return;
    }
    if let Some(head) = list_head(view) {
        if symbol_in(head, &READERS) {
            found.push(UnguardedRead {
                span: view.span,
                operator: paredit_core_syntax::view_query::unqualified(head).to_ascii_lowercase(),
            });
        }
    }
    for child in &view.children {
        walk(child, found);
    }
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
        for read in examine(view) {
            sink.report(
                read.span,
                format!(
                    "{} runs with *read-eval* at its default of true, so a #. in the input is \
                     executed while reading; bind *read-eval* to nil around it",
                    read.operator
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

    fn operators(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    #[test]
    fn flags_an_unguarded_read() {
        assert_eq!(operators("(defun load-data (s) (read s))"), vec!["read"]);
    }

    #[test]
    fn flags_every_reader_in_the_family() {
        assert_eq!(
            operators("(defun f (s) (read-from-string s))"),
            vec!["read-from-string"]
        );
        assert_eq!(
            operators("(defun f (s) (read-preserving-whitespace s))"),
            vec!["read-preserving-whitespace"]
        );
    }

    #[test]
    fn does_not_flag_a_read_under_a_guard() {
        assert!(operators("(defun load-data (s) (let ((*read-eval* nil)) (read s)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_read_deep_under_a_guard() {
        assert!(
            operators("(defun f (s) (let ((*read-eval* nil)) (loop repeat 3 collect (read s))))")
                .is_empty()
        );
    }

    #[test]
    fn a_binding_to_t_is_not_a_guard() {
        assert_eq!(
            operators("(defun f (s) (let ((*read-eval* t)) (read s)))"),
            vec!["read"]
        );
    }

    #[test]
    fn a_binding_of_another_special_is_not_a_guard() {
        assert_eq!(
            operators("(defun f (s) (let ((*print-base* 16)) (read s)))"),
            vec!["read"]
        );
    }

    #[test]
    fn a_read_outside_a_definition_reports_nothing() {
        // A top-level `(read)` is a REPL idiom, not a data path.
        assert!(operators("(read s)").is_empty());
    }

    #[test]
    fn several_unguarded_reads_are_all_reported() {
        assert_eq!(
            operators("(defun f (a b) (list (read a) (read b)))"),
            vec!["read", "read"]
        );
    }

    #[test]
    fn a_guard_covers_only_its_own_subtree() {
        assert_eq!(
            operators("(defun f (a b) (list (let ((*read-eval* nil)) (read a)) (read b)))"),
            vec!["read"]
        );
    }
}
