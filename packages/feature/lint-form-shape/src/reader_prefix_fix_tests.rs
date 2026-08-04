//! That a fix keeps the matched form's *own* reader prefix, per rule.
//!
//! PR #127 changed 42 rules from `RuleFix::single(view.span, …)` to
//! `view.content_span`, because `span` begins at the form's own reader prefixes
//! and replacing it deletes them. Five rules in this package were missed:
//! `coerce-to-t`, `redundant-apply`, `values-list-of-list`, `manual-pushnew`
//! and `nested-char-case`. Measured with the binary built from the parent
//! commit, each one turned a `` `(…) `` into an unparenthesized rewrite with no
//! backquote, and SBCL could no longer read the file:
//!
//! ```text
//! `(coerce ,x t)                    ->  ,x
//! `(apply #'g (list ,a))            ->  (g ,a)
//! `(values-list (list ,a))          ->  (values ,a)
//! `(setf l (adjoin ,v l))           ->  (pushnew ,v l)
//! `(char-upcase (char-downcase ,c)) ->  (char-upcase ,c)
//! ```
//!
//! # Why the existing tests all passed
//!
//! [`crate::quote_guard_tests`] already runs each of these rules through the
//! real dispatch and asserts rewritten source, and it did not catch this — its
//! sources nest the matched form *inside* a template, so the form itself
//! carries no prefix and `span` and `content_span` coincide. Every domain test
//! is blind to it for a stronger reason: a domain never applies a fix, so a
//! replacement region that starts one byte early is invisible to every
//! assertion about spans and counts. The distinguishing shape is the form
//! carrying the prefix *itself*, which is what the table below writes.
//!
//! # Why the hard-quote guard is not this
//!
//! PR #130's guard fires on `'`. It is orthogonal to this defect twice over: it
//! does nothing under a `` ` ``, and a quasiquote template is live code the
//! rule *should* still rewrite. So the requirement here is to keep the
//! backquote and perform the rewrite, not to go quiet. `carries_a_hard_quote`
//! is retained per rule as the control that says which of the two is which.
//!
//! # Where the expected strings come from
//!
//! Every one is the output of `paredit fix apply --rule <rule>` on the source
//! beside it, and every source and every output was then read by SBCL itself:
//! 30 of 30 rows read before and after. Under the parent commit the five
//! `carries_a_backquote` outputs were the only unreadable ones, 5 of 30, and
//! the `no_prefix` and `carries_a_hard_quote` rows were byte-identical to what
//! they are now — which is what makes the table a prefix test rather than a
//! rewrite test.

/// Declares, per rule, one test per reader-prefix shape the rule's fix can be
/// reached under.
///
/// A shared source per shape would let one rule's regression hide behind
/// another's fix; one module per rule keeps "this rule kept the prefix" and
/// "some rule kept a prefix" from being the same observation.
macro_rules! reader_prefix_tests {
    ($($name:ident: $rule:literal,
        no_prefix $plain:literal => $plain_fixed:literal,
        backquote $backquote:literal => $backquote_fixed:literal,
        unquote $unquote:literal => $unquote_fixed:literal,
        splicing $splicing:literal => $splicing_fixed:literal,
        sharp_quote $sharp:literal => $sharp_fixed:literal,
        hard_quote $hard:literal;)*) => {
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

                /// The control. Without it a rule that stopped firing
                /// altogether would pass every other test here.
                #[test]
                fn an_unprefixed_form_is_rewritten() {
                    assert_eq!(fixed($plain), vec![$plain_fixed.to_owned()]);
                }

                /// The measured corruption: the backquote must survive, and
                /// the rewrite must still happen. Dropping it left the commas
                /// underneath outside any backquote and the file unreadable.
                #[test]
                fn carries_a_backquote() {
                    assert_eq!(fixed($backquote), vec![$backquote_fixed.to_owned()]);
                }

                /// `,` — dropping it silently promotes a substituted value to
                /// a literal, which still parses and so is never caught by a
                /// round trip.
                #[test]
                fn carries_an_unquote() {
                    assert_eq!(fixed($unquote), vec![$unquote_fixed.to_owned()]);
                }

                /// `,@` — dropping it splices one form where a list was meant.
                #[test]
                fn carries_a_splicing_unquote() {
                    assert_eq!(fixed($splicing), vec![$splicing_fixed.to_owned()]);
                }

                /// `#'` — dropping it turns a function designator into a call.
                #[test]
                fn carries_a_sharp_quote() {
                    assert_eq!(fixed($sharp), vec![$sharp_fixed.to_owned()]);
                }

                /// The prefix that must *not* be preserved-and-rewritten but
                /// declined outright, since the form is a datum. This is PR
                /// #130's guard, asserted here so a future widening of the
                /// span cannot quietly re-enable a rewrite of literal data.
                #[test]
                fn carries_a_hard_quote() {
                    assert_eq!(fixed($hard), Vec::<String>::new());
                }
            }
        )*
    };
}

reader_prefix_tests! {
    coerce_to_t: "coerce-to-t",
        no_prefix "(defun f (x) (coerce x t))"
            => "(defun f (x) x)",
        backquote "(defmacro m (x) `(coerce ,x t))"
            => "(defmacro m (x) `,x)",
        unquote "(defmacro m (x) `(list ,(coerce x t)))"
            => "(defmacro m (x) `(list ,x))",
        splicing "(defmacro m (x) `(list ,@(coerce x t)))"
            => "(defmacro m (x) `(list ,@x))",
        sharp_quote "(defun f (x) (g #'(coerce x t)))"
            => "(defun f (x) (g #'x))",
        hard_quote "(defparameter *d* '(coerce x t))";
    redundant_apply: "redundant-apply",
        no_prefix "(defun f (x) (apply #'g (list a)))"
            => "(defun f (x) (g a))",
        backquote "(defmacro m (x) `(apply #'g (list ,a)))"
            => "(defmacro m (x) `(g ,a))",
        unquote "(defmacro m (x) `(list ,(apply #'g (list a))))"
            => "(defmacro m (x) `(list ,(g a)))",
        splicing "(defmacro m (x) `(list ,@(apply #'g (list a))))"
            => "(defmacro m (x) `(list ,@(g a)))",
        sharp_quote "(defun f (x) (g #'(apply #'g (list a))))"
            => "(defun f (x) (g #'(g a)))",
        hard_quote "(defparameter *d* '(apply #'g (list a)))";
    values_list_of_list: "values-list-of-list",
        no_prefix "(defun f (x) (values-list (list a)))"
            => "(defun f (x) (values a))",
        backquote "(defmacro m (x) `(values-list (list ,a)))"
            => "(defmacro m (x) `(values ,a))",
        unquote "(defmacro m (x) `(list ,(values-list (list a))))"
            => "(defmacro m (x) `(list ,(values a)))",
        splicing "(defmacro m (x) `(list ,@(values-list (list a))))"
            => "(defmacro m (x) `(list ,@(values a)))",
        sharp_quote "(defun f (x) (g #'(values-list (list a))))"
            => "(defun f (x) (g #'(values a)))",
        hard_quote "(defparameter *d* '(values-list (list a)))";
    manual_pushnew: "manual-pushnew",
        no_prefix "(defun f (x) (setf l (adjoin v l)))"
            => "(defun f (x) (pushnew v l))",
        backquote "(defmacro m (x) `(setf l (adjoin ,v l)))"
            => "(defmacro m (x) `(pushnew ,v l))",
        unquote "(defmacro m (x) `(list ,(setf l (adjoin v l))))"
            => "(defmacro m (x) `(list ,(pushnew v l)))",
        splicing "(defmacro m (x) `(list ,@(setf l (adjoin v l))))"
            => "(defmacro m (x) `(list ,@(pushnew v l)))",
        sharp_quote "(defun f (x) (g #'(setf l (adjoin v l))))"
            => "(defun f (x) (g #'(pushnew v l)))",
        hard_quote "(defparameter *d* '(setf l (adjoin v l)))";
    nested_char_case: "nested-char-case",
        no_prefix "(defun f (x) (char-upcase (char-downcase c)))"
            => "(defun f (x) (char-upcase c))",
        backquote "(defmacro m (x) `(char-upcase (char-downcase ,c)))"
            => "(defmacro m (x) `(char-upcase ,c))",
        unquote "(defmacro m (x) `(list ,(char-upcase (char-downcase c))))"
            => "(defmacro m (x) `(list ,(char-upcase c)))",
        splicing "(defmacro m (x) `(list ,@(char-upcase (char-downcase c))))"
            => "(defmacro m (x) `(list ,@(char-upcase c)))",
        sharp_quote "(defun f (x) (g #'(char-upcase (char-downcase c))))"
            => "(defun f (x) (g #'(char-upcase c)))",
        hard_quote "(defparameter *d* '(char-upcase (char-downcase c)))";
}

/// `coerce-to-t` alone puts an *operand's own source* where the whole form
/// stood, rather than a freshly parenthesized `(head …)`. So it alone can be
/// asked to emit a leading splicing unquote, and re-spanning it to
/// `content_span` is not enough — the rewrite has to be declined.
///
/// The other four are unreachable this way by construction: each builds its
/// replacement as `format!("(… )")`, so whatever the operands carry ends up
/// *inside* parentheses where a splice is well-formed. The
/// `carries_a_splicing_unquote` rows above are the evidence for that, and they
/// are why this module is not a fifth entry in the table.
mod coerce_to_t_spliced_operand {
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
        &crate::coerce_to_t::rule::META,
        &crate::coerce_to_t::rule::RULE,
    )];

    fn fixed(source: &str) -> Vec<String> {
        run_rule_fixed(&ENTRIES, source)
            .into_iter()
            .map(|(_, fixed)| fixed)
            .collect()
    }

    /// `` `(coerce ,@xs t) `` re-spanned to `content_span` would produce
    /// `` `,@xs ``, which SBCL refuses to read: a splicing unquote has no list
    /// to splice into there. Confirmed against SBCL rather than assumed.
    ///
    /// The operand count is expansion-dependent besides — `xs` expanding to
    /// anything but one form means this was never the two-operand `coerce` the
    /// rule reasons about — so there is no rewrite to emit under either
    /// objection.
    #[test]
    fn a_spliced_operand_under_a_backquote_is_declined() {
        assert_eq!(
            fixed("(defmacro m (xs) `(coerce ,@xs t))"),
            Vec::<String>::new()
        );
    }

    /// `,.` splices exactly as `,@` does and `` ` `` rejects it identically,
    /// both verified against SBCL. When this test was written the parser
    /// labelled `,.` as `ReaderPrefix::Unquote`, so a guard written against the
    /// enum rather than the replacement text would have inherited the mislabel
    /// and corrupted this source. PR #136 fixed the parser, so the enum would
    /// answer correctly today — this case is kept because it pins the
    /// *behaviour*, which must hold whichever key the guard uses.
    #[test]
    fn an_unquote_dot_operand_under_a_backquote_is_declined() {
        assert_eq!(
            fixed("(defmacro m (xs) `(coerce ,.xs t))"),
            Vec::<String>::new()
        );
    }

    /// The guard's negative control, and the reason it is keyed on the
    /// operand's *leading* characters. A plain `,` operand is not a splice, and
    /// suppressing it would silently abandon the ordinary macro-template case
    /// this whole change exists to keep working.
    #[test]
    fn a_plain_unquote_operand_is_still_fixed() {
        assert_eq!(
            fixed("(defmacro m (x) `(coerce ,x t))"),
            vec!["(defmacro m (x) `,x)".to_owned()]
        );
    }

    /// The other half of that control: a splice *inside* the operand rather
    /// than leading it is well-formed once the operand keeps its parentheses,
    /// so the guard must not reach it.
    #[test]
    fn a_splice_nested_inside_the_operand_is_still_fixed() {
        assert_eq!(
            fixed("(defmacro m (xs) `(coerce (g ,@xs) t))"),
            vec!["(defmacro m (xs) `(g ,@xs))".to_owned()]
        );
    }
}
