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
//! `lint-form-shape` uses: several rules in *this* workspace match `progn`,
//! `prog1` and `prog2`, so a `progn` wrapper would be a second subject rather
//! than inert scaffolding.
//!
//! `car-nthcdr` is deliberately not guarded and so is absent here: all 5 of its
//! hard-quoted findings in that corpus sit inside SBCL `deftransform` /
//! `define-source-transform` templates, whose bodies are spliced back into the
//! compiler as source. A guard there would cost 5 correct fixes and prevent no
//! corruption at all.

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
    append_list_to_cons: "append-list-to-cons",
        template "(defmacro m () `(wrap (append (list al) ar)))",
        becomes "(defmacro m () `(wrap (cons al ar)))",
        inert "(defparameter *d* '(wrap (append (list al) ar)))";
    append_nil: "append-nil",
        template "(defmacro m () `(wrap (append c nil)))",
        becomes "(defmacro m () `(wrap (copy-list c)))",
        inert "(defparameter *d* '(wrap (append c nil)))";
    car_reverse: "car-reverse",
        template "(defmacro m () `(wrap (car (reverse crx))))",
        becomes "(defmacro m () `(wrap (car (last crx))))",
        inert "(defparameter *d* '(wrap (car (reverse crx))))";
    cons_to_list: "cons-to-list",
        template "(defmacro m () `(wrap (cons t nil)))",
        becomes "(defmacro m () `(wrap (list t)))",
        inert "(defparameter *d* '(wrap (cons t nil)))";
    double_reverse: "double-reverse",
        template "(defmacro m () `(wrap (reverse (reverse dr))))",
        becomes "(defmacro m () `(wrap (copy-seq dr)))",
        inert "(defparameter *d* '(wrap (reverse (reverse dr))))";
    last_default_count: "last-default-count",
        template "(defmacro m () `(wrap (last ldc 1)))",
        becomes "(defmacro m () `(wrap (last ldc)))",
        inert "(defparameter *d* '(wrap (last ldc 1)))";
    list_star_nil: "list-star-nil",
        template "(defmacro m () `(wrap (list* 0 nil nil)))",
        becomes "(defmacro m () `(wrap (list 0 nil)))",
        inert "(defparameter *d* '(wrap (list* 0 nil nil)))";
    list_star_to_cons: "list-star-to-cons",
        template "(defmacro m () `(wrap (list* 2 2)))",
        becomes "(defmacro m () `(wrap (cons 2 2)))",
        inert "(defparameter *d* '(wrap (list* 2 2)))";
    nth_constant_index: "nth-constant-index",
        template "(defmacro m () `(wrap (nth 9 zs)))",
        becomes "(defmacro m () `(wrap (tenth zs)))",
        inert "(defparameter *d* '(wrap (nth 9 zs)))";
    nthcdr_small_index: "nthcdr-small-index",
        template "(defmacro m () `(wrap (nthcdr 2 x)))",
        becomes "(defmacro m () `(wrap (cddr x)))",
        inert "(defparameter *d* '(wrap (nthcdr 2 x)))";
    nthcdr_zero: "nthcdr-zero",
        template "(defmacro m () `(wrap (nthcdr 0 nz)))",
        becomes "(defmacro m () `(wrap nz))",
        inert "(defparameter *d* '(wrap (nthcdr 0 nz)))";
    redundant_count_nil: "redundant-count-nil",
        template "(defmacro m () `(wrap (remove rcn lst :count nil)))",
        becomes "(defmacro m () `(wrap (remove rcn lst)))",
        inert "(defparameter *d* '(wrap (remove rcn lst :count nil)))";
    redundant_end_nil: "redundant-end-nil",
        template "(defmacro m () `(wrap (find ren lst :end nil)))",
        becomes "(defmacro m () `(wrap (find ren lst)))",
        inert "(defparameter *d* '(wrap (find ren lst :end nil)))";
    redundant_eql_test: "redundant-eql-test",
        template "(defmacro m () `(wrap (subsetp a b :test #'eql)))",
        becomes "(defmacro m () `(wrap (subsetp a b)))",
        inert "(defparameter *d* '(wrap (subsetp a b :test #'eql)))";
    redundant_from_end_nil: "redundant-from-end-nil",
        template "(defmacro m () `(wrap (find rfe lst :from-end nil)))",
        becomes "(defmacro m () `(wrap (find rfe lst)))",
        inert "(defparameter *d* '(wrap (find rfe lst :from-end nil)))";
    redundant_identity_key: "redundant-identity-key",
        template "(defmacro m () `(wrap (sort rik #'< :key #'identity)))",
        becomes "(defmacro m () `(wrap (sort rik #'<)))",
        inert "(defparameter *d* '(wrap (sort rik #'< :key #'identity)))";
    redundant_start_zero: "redundant-start-zero",
        template "(defmacro m () `(wrap (find rsz lst :start 0)))",
        becomes "(defmacro m () `(wrap (find rsz lst)))",
        inert "(defparameter *d* '(wrap (find rsz lst :start 0)))";
    single_operand_list_op: "single-operand-list-op",
        template "(defmacro m () `(wrap (list* t)))",
        becomes "(defmacro m () `(wrap t))",
        inert "(defparameter *d* '(wrap (list* t)))";
    subseq_zero: "subseq-zero",
        template "(defmacro m () `(wrap (subseq a 0)))",
        becomes "(defmacro m () `(wrap (copy-seq a)))",
        inert "(defparameter *d* '(wrap (subseq a 0)))";
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

    static NTHCDR: [RuleEntry; 1] = [RuleEntry::new(
        &crate::nthcdr_zero::rule::META,
        &crate::nthcdr_zero::rule::RULE,
    )];

    /// A second rule for the unquote cases, because `nthcdr-zero` replaces the
    /// matched node's whole span and so deletes an `,` of its own — a
    /// reader-prefix defect that is *not* what this file is about, and that
    /// asserting here would silently bless. `cons-to-list` replaces
    /// `content_span` and keeps its comma, so it can state the intended
    /// behaviour without also pinning a bug.
    static CONS: [RuleEntry; 1] = [RuleEntry::new(
        &crate::cons_to_list::rule::META,
        &crate::cons_to_list::rule::RULE,
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
            fixed(&NTHCDR, "(defparameter *d* '(nthcdr 0 nz))"),
            Vec::<String>::new()
        );
        assert_eq!(
            fixed(&CONS, "(defparameter *d* '(wrap (cons v nil)))"),
            Vec::<String>::new()
        );
    }

    /// The same datum spelled long-hand. A guard that only understood the `'`
    /// reader prefix would rewrite this one.
    #[test]
    fn a_long_hand_quote_form_is_inert() {
        assert_eq!(
            fixed(&NTHCDR, "(defparameter *d* (quote (wrap (nthcdr 0 nz))))"),
            Vec::<String>::new()
        );
        assert_eq!(
            fixed(&CONS, "(defparameter *d* (quote (wrap (cons v nil))))"),
            Vec::<String>::new()
        );
    }

    /// The control for the tests above: with no quote anywhere, the rule still
    /// fixes. Without this, a guard that suppressed everything would pass all
    /// of them.
    #[test]
    fn the_same_form_is_still_fixed_as_plain_code() {
        assert_eq!(
            fixed(&NTHCDR, "(defun f (nz) (nthcdr 0 nz))"),
            vec!["(defun f (nz) nz)".to_owned()]
        );
        assert_eq!(
            fixed(&CONS, "(defun f (v) (cons v nil))"),
            vec!["(defun f (v) (list v))".to_owned()]
        );
    }

    /// A hard quote never clears, so an inner comma does not re-enter code —
    /// the shape a single depth counter reads wrongly.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_re_enter_code() {
        assert_eq!(
            fixed(&NTHCDR, "(defparameter *d* '(a ,(nthcdr 0 nz)))"),
            Vec::<String>::new()
        );
        assert_eq!(
            fixed(&CONS, "(defparameter *d* '(a ,(cons v nil)))"),
            Vec::<String>::new()
        );
    }

    /// The symmetric case, and the reason the guard reads `hard` rather than
    /// "has an ancestor that is data": a comma inside a *quasiquote* escapes
    /// back to code, and that code really is fixed — comma and all.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_still_code() {
        assert_eq!(
            fixed(&CONS, "(defmacro m (v) `(a ,(cons v nil)))"),
            vec!["(defmacro m (v) `(a ,(list v)))".to_owned()]
        );
    }
}
