//! `condition-type-datum-with-string-initarg`: a format string where an initarg
//! name belongs.
//!
//! `error`, `signal`, `warn` and `cerror` all take a *condition designator*: a
//! `datum` and some `arguments`. What `arguments` means depends entirely on what
//! `datum` is (CLHS 9.1.4.1, "Signaling"):
//!
//! - `datum` is a **format control** — a string — and `arguments` are its format
//!   arguments. `(error "boom ~A" x)` is this case, and it is correct.
//! - `datum` is a **symbol naming a condition type** and `arguments` are
//!   alternating **initarg names and values**, handed to `make-condition`.
//!   `(error 'my-error :code 42)` is this case, and it is correct.
//!
//! The mistake is to mix them: `(error 'my-error "boom ~A" x)`, which reads like
//! "signal `my-error` with this message" and is nothing of the kind. `"boom ~A"`
//! lands in an *initarg name* position, and an initarg name is a symbol. A
//! string can never be one.
//!
//! # What SBCL 2.6.0 actually does
//!
//! Both spellings were run. Neither is diagnosed at compile time.
//!
//! ```text
//! (error 'my-error "boom")          => TYPE=SIMPLE-ERROR
//!                                      MSG=odd-length initializer list: ("boom").
//! (error 'my-error "boom ~A" 42)    => TYPE=MY-ERROR  MSG=boom
//! (error 'my-error :code 42)        => TYPE=MY-ERROR  MSG=boom     [correct]
//! (error "boom ~A" 42)              => TYPE=SIMPLE-ERROR MSG=boom 42 [correct]
//! (compile nil '(lambda () (error 'my-error "boom")))
//!                                   => warnings-p=NIL failure-p=NIL
//! ```
//!
//! Both are bugs and they fail *differently*, which is why the rule reports both
//! rather than only the loud one:
//!
//! - An **odd** number of arguments signals `simple-error` — "odd-length
//!   initializer list" — so the condition the author named is never signalled at
//!   all, and a `handler-case` on `my-error` does not run.
//! - An **even** number is worse: SBCL accepts the string as an unknown initarg
//!   name, discards it, and signals `my-error` with its *static* `:report`. The
//!   program works, the message the author wrote is silently gone, and nothing
//!   anywhere says so.
//!
//! `warnings-p=NIL failure-p=NIL` is the whole justification for a lint rule
//! here: the compiler will not tell you, in either case.
//!
//! # Why this is sharp
//!
//! The rule asks one question — is the argument immediately after a literally
//! quoted condition-type datum a string literal? — and initarg names are
//! symbols, so the answer is never a matter of degree. Note that CLHS does *not*
//! require an initarg name to be a *keyword*; `(defclass … (x :initarg x))` is
//! legal. That is why the test is "is a string", not "is not a keyword": the
//! latter would report the legal non-keyword spelling.
//!
//! Report-only. Whether the author wanted `:format-control` on a
//! `simple-condition` subtype, a different initarg, or a plain
//! `(error "boom ~A" x)` is not something the form records.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::support::{is_string_literal, quoted_symbol};

pub const META: RuleMeta = RuleMeta::new(
    "condition-type-datum-with-string-initarg",
    RuleCategory::Conditions,
    Severity::Error,
    "a signalling form naming a condition type and then passing a string, which lands in an \
     initarg-name position rather than being a message",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "When the datum of `error`/`signal`/`warn`/`cerror` is a symbol naming a condition type, \
         the remaining arguments are alternating initarg names and values — not format arguments. \
         A string in an initarg-name position is either an `odd-length initializer list` error, \
         which means the named condition is never signalled, or, with an even argument count, an \
         unknown initarg that is silently discarded. SBCL diagnoses neither at compile time.",
    )
    .with_example(
        "(error 'parse-failed \"bad token ~A\" token)",
        "(error 'parse-failed :token token)",
    )
    .with_caveat(
        "Only a *literally quoted* type name is examined. `(error datum args)` with a computed \
         datum could be a format control at run time, and nothing is claimed about it.",
    ),
);

/// The signalling operators whose datum is their **first** argument
/// (CLHS `error`, `signal`, `warn`, and `make-condition`'s type).
const DATUM_FIRST: [&str; 4] = ["error", "signal", "warn", "make-condition"];

/// `cerror`, whose *first* argument is the continue-format-control and whose
/// datum is therefore the **second** (CLHS `cerror`).
///
/// Getting this index wrong would read the continue-format-control — which is a
/// string by design, and correct — as the defect this rule reports.
const CERROR: &str = "cerror";

const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("error"),
    NormalizedHead::new("cerror"),
    NormalizedHead::new("signal"),
    NormalizedHead::new("warn"),
    NormalizedHead::new("make-condition"),
];

/// One misplaced string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringInitarg {
    /// The span of the string, which is the thing that is in the wrong place.
    pub span: ByteSpan,
    /// The condition type the call named.
    pub condition_type: String,
}

/// The index of the `datum` argument for a signalling operator, or `None` when
/// the head is not one.
fn datum_index(view: &ExpressionView) -> Option<usize> {
    let head = list_head(view)?;
    let name = crate::support::normalized_symbol(head);
    if DATUM_FIRST.contains(&name.as_str()) {
        return Some(1);
    }
    (name == CERROR).then_some(2)
}

/// The misplaced string of one signalling call, if it has one.
///
/// Deliberately local: it reads at most two children of the node the dispatcher
/// already handed over, and never consults the tree.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<StringInitarg> {
    // No `is_paren_list` check: `list_head` is defined as
    // `is_paren_list(view).then(|| atom_child(view, 0)).flatten()`, so
    // `datum_index` already fails on anything that is not a paren list. A
    // separate guard was written, mutation-tested, and killed no test.
    let datum_index = datum_index(view)?;
    // The datum has to name a condition type *literally*. A computed datum, or
    // a plain string, is the format-control case and is correct.
    let condition_type = quoted_symbol(view.children.get(datum_index)?)?;
    let first_argument = view.children.get(datum_index + 1)?;
    // A reader conditional needs no guard here: `#+sbcl "boom"` folds into one
    // atom whose text carries the prefix, so `is_string_literal` is already
    // false for it. See `support::is_string_literal`.
    if !is_string_literal(first_argument) {
        return None;
    }
    Some(StringInitarg {
        span: first_argument.span,
        condition_type,
    })
}

/// Whether the head is `warn`, whose message names a warning rather than an
/// error — used only to phrase the finding.
fn is_warn(view: &ExpressionView) -> bool {
    list_head(view)
        .map(crate::support::normalized_symbol)
        .is_some_and(|name| name == "warn")
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // The cheap domain check first, and `is_unevaluated_at` — which reaches
        // `root_view()` — only once there is something to report.
        let Some(found) = examine(view) else {
            return Ok(());
        };
        if crate::support::is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        let signalled = if is_warn(view) { "warned" } else { "signalled" };
        sink.report(
            found.span,
            format!(
                "`{}` names a condition type, so this string lands in an initarg-name position \
                 rather than being a message: an odd argument count makes it an odd-length \
                 initializer list, so `{}` is never {}, and an even one silently discards it",
                found.condition_type, found.condition_type, signalled
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// Every finding of a whole source, through the evaluated walk, so the
    /// quoting tests below exercise the same model the rule uses.
    fn types(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let mut found = Vec::new();
        crate::support::for_each_evaluated_subview(&tree.root_view(), |view| {
            if let Some(item) = examine(view) {
                found.push(item.condition_type);
            }
        });
        found
    }

    #[test]
    fn flags_an_odd_length_initializer_list() {
        assert_eq!(types("(error 'my-error \"boom\")"), vec!["my-error"]);
    }

    /// The silent one. SBCL accepts it, discards the string, and signals the
    /// condition with its static report.
    #[test]
    fn flags_the_even_length_form_that_is_silently_discarded() {
        assert_eq!(types("(error 'my-error \"boom ~A\" 42)"), vec!["my-error"]);
    }

    #[test]
    fn flags_signal_and_warn_and_make_condition() {
        assert_eq!(types("(signal 'my-error \"boom\")"), vec!["my-error"]);
        assert_eq!(types("(warn 'my-warning \"boom\")"), vec!["my-warning"]);
        assert_eq!(
            types("(make-condition 'my-error \"boom\")"),
            vec!["my-error"]
        );
    }

    /// `cerror`'s datum is its *second* argument. Reading index 1 would report
    /// the continue-format-control, which is a string by design.
    #[test]
    fn reads_cerror_datum_at_the_second_argument() {
        assert_eq!(
            types("(cerror \"Retry.\" 'my-error \"boom\")"),
            vec!["my-error"]
        );
    }

    #[test]
    fn does_not_flag_a_correct_cerror() {
        assert!(types("(cerror \"Retry.\" 'my-error :code 42)").is_empty());
        assert!(types("(cerror \"Retry.\" \"boom ~A\" 42)").is_empty());
    }

    #[test]
    fn does_not_flag_a_format_control_datum() {
        assert!(types("(error \"boom ~A\" 42)").is_empty());
        assert!(types("(warn \"careful ~A\" x)").is_empty());
    }

    #[test]
    fn does_not_flag_correct_initargs() {
        assert!(types("(error 'my-error :code 42)").is_empty());
        assert!(
            types("(error 'simple-error :format-control \"boom ~A\" :format-arguments (list x))")
                .is_empty()
        );
    }

    /// CLHS does not require an initarg name to be a keyword, so a non-keyword
    /// symbol in that position is legal and must not be reported. This is why
    /// the test is "is a string" rather than "is not a keyword".
    #[test]
    fn does_not_flag_a_non_keyword_initarg_name() {
        assert!(types("(error 'my-error code 42)").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_or_unquoted_datum() {
        assert!(types("(error condition-object)").is_empty());
        assert!(types("(error (make-condition 'my-error) )").is_empty());
        assert!(types("(signal datum \"boom\")").is_empty());
    }

    /// A `define-condition`'s supertype list `(error)` is a paren list whose
    /// head is `error`, so the head index hands it to this rule as if it were a
    /// call. It survives because child 1 does not exist — but that is an
    /// accident worth pinning, since a future change reading child 1 with a
    /// default would start reporting every condition definition in the file.
    #[test]
    fn does_not_flag_a_define_condition_supertype_list() {
        assert!(
            types("(define-condition app-error (error) ((code :initarg :code)) (:report \"app\"))")
                .is_empty()
        );
        // The dangerous variant: a supertype list with a second element.
        assert!(types("(define-condition app-error (error warning) ())").is_empty());
    }

    #[test]
    fn does_not_flag_a_call_with_no_arguments_after_the_datum() {
        assert!(types("(error 'my-error)").is_empty());
        assert!(types("(signal 'my-error)").is_empty());
    }

    #[test]
    fn reads_a_package_qualified_type_by_its_name() {
        assert_eq!(types("(error 'app::my-error \"boom\")"), vec!["my-error"]);
    }

    #[test]
    fn reads_the_long_hand_quote_form() {
        assert_eq!(types("(error (quote my-error) \"boom\")"), vec!["my-error"]);
    }

    /// `#+sbcl "boom"` folds into one atom carrying its reader prefix, so which
    /// form it stands for is build-dependent and no claim is made.
    #[test]
    fn does_not_flag_a_reader_conditional_argument() {
        assert_eq!(
            types("(error 'my-error \"boom\")"),
            vec!["my-error"],
            "the same call without the reader conditional is flagged"
        );
        assert!(types("(error 'my-error #+sbcl \"boom\")").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data() {
        assert!(types("'(error 'my-error \"boom\")").is_empty());
        assert!(types("(quote (error 'my-error \"boom\"))").is_empty());
        assert!(types("`(error 'my-error \"boom\")").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            types("`(progn ,(error 'my-error \"boom\"))"),
            vec!["my-error"]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(types("(format t \"(error 'my-error \\\"boom\\\")\")").is_empty());
    }

    #[test]
    fn the_finding_points_at_the_string_not_the_whole_call() {
        let tree =
            SyntaxTree::parse_with_dialect("(error 'my-error \"boom\")", Dialect::CommonLisp)
                .expect("parse");
        let call = &tree.root_view().children[0];
        let found = examine(call).expect("a finding");
        let text = &"(error 'my-error \"boom\")"[found.span.start().get()..found.span.end().get()];
        assert_eq!(text, "\"boom\"");
    }
}
