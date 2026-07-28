//! `global-mutation-in-function`: a function assigning to a global.
//!
//! `(defun record (x) (push x *log*))` is a function whose result depends on
//! every previous call and whose behaviour under two threads is undefined
//! without a lock. That is sometimes exactly what was wanted; it is never
//! invisible, and the point of the rule is that in a 2,000-line file it is.
//!
//! The global is identified by its *name*: `*earmuffs*` is the convention every
//! Common Lisp codebase uses for a special variable, and it is the only signal
//! available without a cross-file view of every `defvar`. That makes the rule
//! precise about what it claims — it reports a name shaped like a special, not
//! a proven special — and the message says so.
//!
//! Report-only. Whether the right answer is a lock, an atomic, a parameter, or
//! nothing at all is a design question.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "global-mutation-in-function",
    RuleCategory::Concurrency,
    Severity::Warning,
    "a function body assigning to a *special-looking* global, which is shared mutable state",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A special variable is visible to every thread and every caller, so assigning to one from \
         inside a function makes the function's result depend on call history and its behaviour \
         under concurrency undefined without external synchronisation.",
    )
    .with_example(
        "(defun record (x) (push x *log*))",
        "(defun record (x log) (cons x log))",
    )
    .with_caveat(
        "A `let`-binding of a special — `(let ((*print-base* 16)) …)` — is not an assignment: it \
         is dynamically scoped, thread-local in every implementation that has threads, and is \
         never reported.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("defgeneric"),
    NormalizedHead::new("lambda"),
];

/// The operators that write to a place.
const MUTATORS: [&str; 10] = [
    "setf", "setq", "incf", "decf", "push", "pushnew", "pop", "rotatef", "shiftf", "remf",
];

/// One assignment to a global inside a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalMutation {
    pub span: ByteSpan,
    pub variable: String,
    pub operator: String,
}

/// Whether `name` is spelled the way Common Lisp spells a special variable.
///
/// Two earmuffs, and something between them: `*` alone is the standard REPL
/// history variable and `**`/`***` are its siblings, none of which is a name a
/// program assigns to in a function.
#[must_use]
pub fn looks_special(name: &str) -> bool {
    let stripped = paredit_core_syntax::view_query::unqualified(name);
    stripped.len() > 2 && stripped.starts_with('*') && stripped.ends_with('*')
}

/// Every global assignment in one definition's body.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<GlobalMutation> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["defun", "defmethod", "defgeneric", "lambda"]) {
        return Vec::new();
    }

    let mut found = Vec::new();
    for_each_subview(view, |subview| {
        let Some(operator) = list_head(subview) else {
            return;
        };
        if !symbol_in(operator, &MUTATORS) {
            return;
        }
        // Every place `setf` and friends take sits at an odd index; reading all
        // of them covers the multi-pair spelling without modelling each
        // operator's arity.
        for (index, argument) in subview.children.iter().enumerate().skip(1) {
            if !matches!(operator_place_positions(operator), Places::All) && index % 2 == 0 {
                continue;
            }
            let Some(name) = atom_text(argument) else {
                continue;
            };
            if !looks_special(name) {
                continue;
            }
            found.push(GlobalMutation {
                span: subview.span,
                variable: name.to_owned(),
                operator: paredit_core_syntax::view_query::unqualified(operator)
                    .to_ascii_lowercase(),
            });
        }
    });
    found
}

/// Which argument positions of a mutator are places.
enum Places {
    /// `setf`/`setq`/`shiftf`/`rotatef` interleave or list places; the odd
    /// positions (or all, for the latter two) are what gets written.
    Odd,
    /// `push`/`pushnew` put the place last, `incf`/`pop` first; reading every
    /// argument is the over-approximation that keeps this from modelling four
    /// separate shapes, and a non-special name is discarded anyway.
    All,
}

fn operator_place_positions(operator: &str) -> Places {
    if symbol_in(operator, &["setf", "setq"]) {
        Places::Odd
    } else {
        Places::All
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
        for mutation in examine(view) {
            sink.report(
                mutation.span,
                format!(
                    "{} writes to {}, whose name marks it as a special variable shared by every \
                     thread and every caller",
                    mutation.operator, mutation.variable
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

    fn variables(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| item.variable)
            .collect()
    }

    #[test]
    fn reads_the_earmuff_convention() {
        assert!(looks_special("*log*"));
        assert!(looks_special("cl:*print-base*"));
        assert!(!looks_special("log"));
        assert!(!looks_special("*"));
        assert!(!looks_special("**"));
        assert!(!looks_special("*log"));
    }

    #[test]
    fn flags_an_assignment_to_a_special_in_a_defun() {
        assert_eq!(
            variables("(defun record (x) (push x *log*))"),
            vec!["*log*"]
        );
        assert_eq!(
            variables("(defun reset () (setf *counter* 0))"),
            vec!["*counter*"]
        );
    }

    #[test]
    fn flags_an_assignment_in_a_method_or_a_lambda() {
        assert_eq!(
            variables("(defmethod run ((x thing)) (incf *calls*))"),
            vec!["*calls*"]
        );
        assert_eq!(
            variables("(lambda () (setq *state* :done))"),
            vec!["*state*"]
        );
    }

    #[test]
    fn does_not_flag_a_local_variable() {
        assert!(variables("(defun f (x) (let ((n 0)) (incf n x) n))").is_empty());
    }

    #[test]
    fn does_not_flag_a_special_only_read() {
        assert!(variables("(defun f () (length *log*))").is_empty());
    }

    #[test]
    fn does_not_flag_a_dynamic_rebinding() {
        // A `let` of a special is scoped and thread-local, not an assignment.
        assert!(variables("(defun f () (let ((*print-base* 16)) (princ 255)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_special_in_a_value_position_of_setf() {
        // `(setf local *default*)` reads the global; it does not write it.
        assert!(variables("(defun f () (setf local *default*))").is_empty());
    }

    #[test]
    fn reads_every_pair_of_a_multi_place_setf() {
        assert_eq!(
            variables("(defun f () (setf local 1 *global* 2))"),
            vec!["*global*"]
        );
    }

    #[test]
    fn an_assignment_outside_a_definition_reports_nothing() {
        // A top-level `(setf *config* …)` is initialisation, not hidden state.
        assert!(variables("(setf *config* (load-config))").is_empty());
    }

    #[test]
    fn finds_an_assignment_nested_deep_in_a_body() {
        assert_eq!(
            variables("(defun f (x) (when x (dolist (y x) (push y *seen*))))"),
            vec!["*seen*"]
        );
    }
}
