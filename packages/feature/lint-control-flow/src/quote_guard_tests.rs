//! The hard-quote guard, per guarded rule, through the real dispatch.
//!
//! Every `Fixability::Fixable` rule in this package rewrites source, and the
//! lint engine's dispatch walks into quoted data like any other subtree. A rule
//! that fires inside `'(…)` therefore rewrites a *data literal* as if it were
//! code, and `--fix` silently corrupts the file — which no round-trip property
//! catches, because a consistently wrong parse is a fixed point of its own
//! round-trip. Each rule now drops such a finding instead of offering a fix.
//!
//! The verdict is read on the `hard` counter alone and never on
//! [`crate::support::is_unevaluated_at`]. The two tests per rule are what pin
//! that distinction down:
//!
//! - `still_fires_inside_a_quasiquote_template` is the negative control, and it
//!   is the reason "is this data?" is the wrong predicate: a `` `(…) ``
//!   template's contents really are emitted as code, so a guard phrased that way
//!   would go quiet on exactly the macro bodies these rules exist to read. It
//!   asserts the *rewritten source* rather than a count, because a fix whose
//!   region starts one byte early deletes a reader prefix while every assertion
//!   about spans and counts still passes.
//! - `is_inert_inside_a_hard_quote` is the positive control: the same form under
//!   a `'` yields no finding at all.
//!
//! Both sources nest the matched form *inside* the quote rather than letting it
//! carry the quote itself, so what they exercise is the guard's walk to an
//! ancestor and not a node-local `reader_prefixes` check.
//!
//! # Where these strings came from
//!
//! Every `template` form below is the exact source text of a finding the rule
//! made on *unquoted code* in a 9,314-file Common Lisp corpus, and every
//! `becomes` string was produced by running the real dispatch over that
//! template — not written by hand. A hand-written expectation is a second guess
//! at the rule's behaviour, and agrees with the first only by luck.
//!
//! The nesting head is `wrap` rather than the `progn` the sibling table in
//! `lint-form-shape` uses: three rules in *this* package match `progn`, `prog1`
//! and `prog2`, so a `progn` wrapper would be a second subject rather than
//! inert scaffolding.
//!
//! # What is absent, and why
//!
//! `redundant-progn` and `redundant-body-progn` were guarded earlier and carry
//! their own quote tests in their own modules; they are not repeated here.
//!
//! `explicit-nil-return` is deliberately **not** guarded. Of its 32 hard-quoted
//! findings in that corpus, 10 are live code rather than data — 5 spliced back
//! as source by `#.` read-eval, and 5 inside SBCL `deftransform` /
//! `defoptimizer` templates, whose bodies the compiler re-reads as source. At
//! 31% exposure a guard there costs a third of its correct fixes, which is the
//! highest rate of any rule measured in either package and more than three
//! times the next.

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
    handler_case_no_clauses: "handler-case-no-clauses",
        template "(defmacro m () `(wrap (handler-case (hcx))))",
        becomes "(defmacro m () `(wrap (hcx)))",
        inert "(defparameter *d* '(wrap (handler-case (hcx))))";
    nested_progn: "nested-progn",
        template "(defmacro m () `(wrap (progn (progn c d))))",
        becomes "(defmacro m () `(wrap (progn c d)))",
        inert "(defparameter *d* '(wrap (progn (progn c d))))";
    prog2_to_progn: "prog2-to-progn",
        template "(defmacro m () `(wrap (prog2 (p2a) (p2b))))",
        becomes "(defmacro m () `(wrap (progn (p2a) (p2b))))",
        inert "(defparameter *d* '(wrap (prog2 (p2a) (p2b))))";
    redundant_prog1: "redundant-prog1",
        template "(defmacro m () `(wrap (prog1 (p1x))))",
        becomes "(defmacro m () `(wrap (p1x)))",
        inert "(defparameter *d* '(wrap (prog1 (p1x))))";
    unwind_protect_no_cleanup: "unwind-protect-no-cleanup",
        template "(defmacro m () `(wrap (unwind-protect (upx))))",
        becomes "(defmacro m () `(wrap (upx)))",
        inert "(defparameter *d* '(wrap (unwind-protect (upx))))";
}

/// The shapes the per-rule table above cannot reach, each one the only thing
/// standing between a plausible-looking guard and a wrong one.
///
/// The table nests every form inside the quote, so it says nothing about a form
/// carrying a quote of its *own*, and it spells every quote `'`, so it says
/// nothing about the long-hand `(quote …)` that macro output and hand-written
/// code both produce.
mod guard_edges {
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static UNWIND: [RuleEntry; 1] = [RuleEntry::new(
        &crate::unwind_protect_no_cleanup::rule::META,
        &crate::unwind_protect_no_cleanup::rule::RULE,
    )];

    fn fixed(entries: &'static [RuleEntry], source: &str) -> Vec<String> {
        run_rule_fixed(entries, source)
            .into_iter()
            .map(|(_, fixed)| fixed)
            .collect()
    }

    /// The matched form carries the quote itself, so a guard that consulted
    /// only the *enclosing* context would call this code and rewrite a datum.
    ///
    /// This is not a hypothetical: it is the exact shape of a measured misfire
    /// in `packages/parse/tests/cl-parser-new-tests.lisp`, where
    /// `'(unwind-protect (x))` is a parser test's *input datum*.
    #[test]
    fn a_form_carrying_its_own_quote_is_inert() {
        assert_eq!(
            fixed(&UNWIND, "(defparameter *d* '(unwind-protect (upx)))"),
            Vec::<String>::new()
        );
    }

    /// The same datum spelled long-hand. A guard that only understood the `'`
    /// reader prefix would rewrite this one.
    #[test]
    fn a_long_hand_quote_form_is_inert() {
        assert_eq!(
            fixed(
                &UNWIND,
                "(defparameter *d* (quote (wrap (unwind-protect (upx)))))"
            ),
            Vec::<String>::new()
        );
    }

    /// The control for the tests above: with no quote anywhere, the rule still
    /// fixes. Without this, a guard that suppressed everything would pass all
    /// of them.
    #[test]
    fn the_same_form_is_still_fixed_as_plain_code() {
        assert_eq!(
            fixed(&UNWIND, "(defun f () (unwind-protect (upx)))"),
            vec!["(defun f () (upx))".to_owned()]
        );
    }

    /// A hard quote never clears, so an inner comma does not re-enter code —
    /// the shape a single depth counter reads wrongly.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_re_enter_code() {
        assert_eq!(
            fixed(&UNWIND, "(defparameter *d* '(a ,(unwind-protect (upx))))"),
            Vec::<String>::new()
        );
    }

    /// The symmetric case, and the reason the guard reads `hard` rather than
    /// "has an ancestor that is data": a comma inside a *quasiquote* escapes
    /// back to code, and that code really is fixed — comma and all.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_still_code() {
        assert_eq!(
            fixed(&UNWIND, "(defmacro m () `(a ,(unwind-protect (upx))))"),
            vec!["(defmacro m () `(a ,(upx)))".to_owned()]
        );
    }
}
