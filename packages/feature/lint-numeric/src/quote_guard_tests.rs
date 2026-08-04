//! The hard-quote guard, per guarded rule, through the real dispatch.
//!
//! Every `Fixability::Fixable` rule in this package rewrites source, and the
//! lint engine's dispatch walks into quoted data like any other subtree. A rule
//! that fires inside `'(…)` therefore rewrites a *data literal* as if it were
//! code, and `--fix` silently corrupts the file — which no round-trip property
//! catches, because a consistently wrong parse is a fixed point of its own
//! round-trip. Each guarded rule now drops such a finding instead of offering a
//! fix for it.
//!
//! The verdict is read on the `hard` counter alone and never on
//! `QuoteState::is_data`. The two tests per rule are what pin that distinction
//! down:
//!
//! - `still_fires_inside_a_quasiquote_template` is the negative control, and it
//!   is the reason `is_data` is the wrong predicate: a `` `(…) `` template's
//!   contents really are emitted as code, so a guard phrased as "is this data?"
//!   would go quiet on exactly the macro bodies these rules exist to read. It
//!   asserts the *rewritten source* rather than a count, because a fix whose
//!   region starts one byte early deletes a reader prefix while every assertion
//!   about spans and counts still passes.
//! - `is_inert_inside_a_hard_quote` is the positive control: the same form under
//!   a `'` yields no finding at all.
//!
//! Both sources nest the matched form *inside* the quote rather than letting it
//! carry the quote itself, so what they exercise is the guard's walk to an
//! ancestor and not a node-local `reader_prefixes` check. The shapes that walk
//! cannot reach live in [`guard_edges`].
//!
//! Every expected string here was produced by running the rule through the real
//! dispatch and printing the spliced source, never by predicting it.
//!
//! # `single-operand-arithmetic` is deliberately not guarded
//!
//! It is absent from the table below and pinned in [`deliberately_unguarded`]
//! instead. Over the 28 827 parsed Common Lisp files this guard was measured on,
//! 20 of its 84 hard-quoted findings (23.8% — 27.0% outside ACL2) are live code:
//! 18 sit inside SBCL `deftransform`/`defoptimizer` templates, whose quoted body
//! is spliced back into the compiler as source, and 2 under an `(eval '…)`. That
//! is a large enough population to estimate the rate on, and at roughly a
//! quarter the guard would be systematically wrong for this rule's idiom, where
//! `'(+ x)` in a transform table is a template rather than a datum.
//!
//! The trade it declines is explicit and measured: guarding it would suppress
//! 64 further data rewrites and cost those 20 correct fixes.

/// Declares, per rule, a one-rule catalogue and the guard's two controls.
///
/// A single shared catalogue would let a second rule firing on one of these
/// sources pass unnoticed; one entry per module keeps "the rule fired" and
/// "some rule fired" from being the same observation.
macro_rules! guarded_rule_tests {
    ($($name:ident: $rule:literal,
        template $template:literal,
        becomes $becomes:literal,
        inert $inert:literal;)*) => {
        $(
            mod $name {
                use crate::support::run_rule_fixed;
                use paredit_core_lint_engine::rule::RuleEntry;

                static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
                    &crate::$name::rule::META,
                    &crate::$name::rule::RULE,
                )];

                /// The source each finding's fix produces, in report order.
                fn fixed(source: &str) -> Vec<String> {
                    run_rule_fixed(&ENTRIES, source)
                        .into_iter()
                        .map(|(_, fixed)| fixed)
                        .collect()
                }

                /// A mis-typed table row would otherwise silently test some
                /// other rule twice and this one not at all.
                #[test]
                fn the_table_row_names_this_rule() {
                    assert_eq!(crate::$name::rule::META.name().as_str(), $rule);
                }

                #[test]
                fn still_fires_inside_a_quasiquote_template() {
                    assert_eq!(fixed($template), vec![$becomes.to_owned()]);
                }

                #[test]
                fn is_inert_inside_a_hard_quote() {
                    assert_eq!(fixed($inert), Vec::<String>::new());
                }
            }
        )*
    };
}

guarded_rule_tests! {
    explicit_step_delta: "explicit-step-delta",
        template "(defmacro m () `(progn (incf x 1)))",
        becomes "(defmacro m () `(progn (incf x)))",
        inert "(defparameter *d* '(progn (incf x 1)))";
    negated_step_delta: "negated-step-delta",
        template "(defmacro m () `(progn (incf x -5)))",
        becomes "(defmacro m () `(progn (decf x 5)))",
        inert "(defparameter *d* '(progn (incf x -5)))";
    nil_comparison: "nil-comparison",
        template "(defmacro m () `(progn (eq x nil)))",
        becomes "(defmacro m () `(progn (null x)))",
        inert "(defparameter *d* '(progn (eq x nil)))";
    redundant_divisor: "redundant-divisor",
        template "(defmacro m () `(progn (floor x 1)))",
        becomes "(defmacro m () `(progn (floor x)))",
        inert "(defparameter *d* '(progn (floor x 1)))";
    sign_comparison: "sign-comparison",
        template "(defmacro m () `(progn (= x 0)))",
        becomes "(defmacro m () `(progn (zerop x)))",
        inert "(defparameter *d* '(progn (= x 0)))";
    verbose_negation: "verbose-negation",
        template "(defmacro m () `(progn (- 0 x)))",
        becomes "(defmacro m () `(progn (- x)))",
        inert "(defparameter *d* '(progn (- 0 x)))";
}

/// The shapes the per-rule table above cannot reach, each one the only thing
/// standing between a plausible-looking guard and a wrong one.
///
/// The table nests every form inside the quote, so it says nothing about a form
/// carrying a quote of its *own*; it spells every quote `'`, so it says nothing
/// about the long-hand `(quote …)` that macro output and hand-written code both
/// produce; and it never puts the reader prefix on the matched form itself, so
/// it says nothing about a fix region that would swallow that prefix.
mod guard_edges {
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static SIGN: [RuleEntry; 1] = [RuleEntry::new(
        &crate::sign_comparison::rule::META,
        &crate::sign_comparison::rule::RULE,
    )];

    static STEP: [RuleEntry; 1] = [RuleEntry::new(
        &crate::explicit_step_delta::rule::META,
        &crate::explicit_step_delta::rule::RULE,
    )];

    static NIL: [RuleEntry; 1] = [RuleEntry::new(
        &crate::nil_comparison::rule::META,
        &crate::nil_comparison::rule::RULE,
    )];

    static DIVISOR: [RuleEntry; 1] = [RuleEntry::new(
        &crate::redundant_divisor::rule::META,
        &crate::redundant_divisor::rule::RULE,
    )];

    static NEGATED: [RuleEntry; 1] = [RuleEntry::new(
        &crate::negated_step_delta::rule::META,
        &crate::negated_step_delta::rule::RULE,
    )];

    fn fixed(entries: &'static [RuleEntry], source: &str) -> Vec<String> {
        run_rule_fixed(entries, source)
            .into_iter()
            .map(|(_, fixed)| fixed)
            .collect()
    }

    /// The matched form carries the quote itself, so a guard that consulted
    /// only the *enclosing* context would call this code and rewrite a datum.
    #[test]
    fn a_form_carrying_its_own_quote_is_inert() {
        assert_eq!(
            fixed(&SIGN, "(defparameter *d* '(= x 0))"),
            Vec::<String>::new()
        );
    }

    /// The same datum spelled long-hand. A guard that only understood the `'`
    /// reader prefix would rewrite this one.
    #[test]
    fn a_long_hand_quote_form_is_inert() {
        assert_eq!(
            fixed(&SIGN, "(defparameter *d* (quote (progn (= x 0))))"),
            Vec::<String>::new()
        );
    }

    /// The control for both tests above: with no quote anywhere, the rule
    /// still fixes. Without this, a guard that suppressed everything would
    /// pass the two of them.
    #[test]
    fn the_same_form_is_still_fixed_as_plain_code() {
        assert_eq!(
            fixed(&SIGN, "(defun f (x) (= x 0))"),
            vec!["(defun f (x) (zerop x))".to_owned()]
        );
    }

    /// The four rules whose fix region used to be the finding's own `span`.
    ///
    /// `span` starts at the form's *own* reader prefixes, so replacing it
    /// deletes them: `` `(incf x 1) `` became `(incf x)` and the commas beneath
    /// a real template would be left outside any backquote, at which point the
    /// file stops reading altogether. The guard cannot catch this — a
    /// quasiquote is not hard-quoted, so the rule correctly still fires — which
    /// is why the fix region is `content_span` and why this is asserted on the
    /// spliced source rather than on a span.
    #[test]
    fn a_fix_on_a_prefixed_form_keeps_the_prefix() {
        assert_eq!(
            fixed(&STEP, "(defmacro m () `(incf x 1))"),
            vec!["(defmacro m () `(incf x))".to_owned()]
        );
        assert_eq!(
            fixed(&NIL, "(defmacro m () `(eq x nil))"),
            vec!["(defmacro m () `(null x))".to_owned()]
        );
        assert_eq!(
            fixed(&DIVISOR, "(defmacro m () `(floor x 1))"),
            vec!["(defmacro m () `(floor x))".to_owned()]
        );
        assert_eq!(
            fixed(&NEGATED, "(defmacro m () `(incf x -5))"),
            vec!["(defmacro m () `(decf x 5))".to_owned()]
        );
    }
}

/// `single-operand-arithmetic` still fires inside a hard quote, on purpose.
///
/// Pinned rather than left implicit: a future guard added here would otherwise
/// pass every test in this file while silently trading this rule's 64 measured
/// data rewrites for the 20 live-code fixes the module doc accounts for. That
/// is a decision to take deliberately and re-measure, not to arrive at by
/// making the package uniform.
mod deliberately_unguarded {
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
        &crate::single_operand_arithmetic::rule::META,
        &crate::single_operand_arithmetic::rule::RULE,
    )];

    fn fixed(source: &str) -> Vec<String> {
        run_rule_fixed(&ENTRIES, source)
            .into_iter()
            .map(|(_, fixed)| fixed)
            .collect()
    }

    #[test]
    fn single_operand_arithmetic_still_fires_inside_a_hard_quote() {
        assert_eq!(
            fixed("(defparameter *d* '(progn (+ x)))"),
            vec!["(defparameter *d* '(progn x))".to_owned()]
        );
    }

    #[test]
    fn single_operand_arithmetic_still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m () `(progn (+ x)))"),
            vec!["(defmacro m () `(progn x))".to_owned()]
        );
    }
}
