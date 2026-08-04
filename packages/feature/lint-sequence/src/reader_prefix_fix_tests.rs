//! That a fix keeps the matched form's *own* reader prefix, per rule.
//!
//! PR #127 changed 42 rules from `RuleFix::single(view.span, …)` to
//! `view.content_span`, because `span` begins at the form's own reader prefixes
//! and replacing it deletes them. PR #133 caught five more in
//! `lint-form-shape`. Five in *this* package were still on `item.span`, and
//! `item.span == view.span`. Measured with the binary built from the parent
//! commit:
//!
//! ```text
//! `(wrap ,(nthcdr 0 nz))         ->  `(wrap nz)
//! `(wrap ,(car (nthcdr 2 x)))    ->  `(wrap (nth 2 x))
//! `(wrap ,(append (list al) ar)) ->  `(wrap (cons al ar))
//! `(wrap ,(car (reverse crx)))   ->  `(wrap (car (last crx)))
//! `(wrap ,(reverse (reverse dr)))->  `(wrap (copy-seq dr))
//! ```
//!
//! # Why a read of the file is not enough to see this
//!
//! Unlike `coerce-to-t` in PR #133, four of these five build their replacement
//! as `format!("(… )")`, so the corrupted output is still *readable*: the comma
//! is simply gone, and what was a substitution has become a literal symbol. The
//! decisive oracle is therefore SBCL's macroexpander, not its reader. Expanding
//! `(m (a b c))` against each definition above:
//!
//! ```text
//! source            (WRAP (A B C))    (WRAP C)             (WRAP ((A B C)))
//! pre-fix output    (WRAP NZ)         (WRAP (NTH 2 X))     (WRAP (CONS AL NIL))
//! ```
//!
//! All five expansions changed under the pre-fix binary and none changes now.
//! That is what "the substitution became a literal symbol" means concretely,
//! and it is invisible to any round trip: the corrupted file parses, and
//! re-parsing it is a fixed point.
//!
//! # Why the existing tests all passed
//!
//! [`crate::quote_guard_tests`] already runs four of these rules through the
//! real dispatch and asserts rewritten source, and it did not catch this — its
//! sources nest the matched form *inside* a template, so the form itself
//! carries no prefix and `span` and `content_span` coincide. Its `guard_edges`
//! module says so out loud: it routes the unquote cases through `cons-to-list`
//! precisely because `nthcdr-zero` "replaces the matched node's whole span and
//! so deletes an `,` of its own". This file is that deferred defect.
//!
//! Every domain test is blind to it for a stronger reason: a domain never
//! applies a fix, so a replacement region that starts one byte early is
//! invisible to every assertion about spans and counts.
//!
//! # Where the expected strings come from
//!
//! Every one is the output of `paredit fix apply --rule <rule>` on the source
//! beside it, and every source and every output was then read by SBCL. Under
//! the parent commit the four spliced-operand outputs were the only unreadable
//! ones, and the `no_prefix` rows were byte-identical to what they are now —
//! which is what makes the table a prefix test rather than a rewrite test.

/// Declares, per rule, one test per reader-prefix shape the rule's fix can be
/// reached under.
///
/// A shared source per shape would let one rule's regression hide behind
/// another's fix; one module per rule keeps "this rule kept the prefix" and
/// "some rule kept a prefix" from being the same observation.
///
/// The hard-quote expectation is an expression rather than a literal because
/// the five rules do not share one policy: four drop the finding under a `'`
/// (PR #134's guard), and `car-nthcdr` is deliberately *not* guarded and
/// rewrites. Both are prefix assertions — the guarded four must not rewrite a
/// datum, and `car-nthcdr` must rewrite it without eating the `'` — and a table
/// that could only express the first would have quietly skipped the one rule
/// whose hard-quote row still moves bytes.
macro_rules! reader_prefix_tests {
    ($($name:ident: $rule:literal,
        no_prefix $plain:literal => $plain_fixed:literal,
        backquote $backquote:literal => $backquote_fixed:literal,
        unquote $unquote:literal => $unquote_fixed:literal,
        splicing $splicing:literal => $splicing_fixed:literal,
        sharp_quote $sharp:literal => $sharp_fixed:literal,
        hard_quote $hard:literal => $hard_expected:expr;)*) => {
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
                /// the rewrite must still happen.
                #[test]
                fn carries_a_backquote() {
                    assert_eq!(fixed($backquote), vec![$backquote_fixed.to_owned()]);
                }

                /// `,` — dropping it silently promotes a substituted value to
                /// a literal, which still parses and so is never caught by a
                /// round trip. SBCL's macroexpander is what catches it.
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

                /// `'` — whichever way this rule resolves it, the `'` itself
                /// must survive. For the four guarded rules that means no
                /// rewrite at all (PR #134); for `car-nthcdr`, which is
                /// deliberately unguarded, it means a rewrite that leaves the
                /// quote where it was rather than promoting a datum to a call.
                #[test]
                fn carries_a_hard_quote() {
                    assert_eq!(fixed($hard), $hard_expected);
                }
            }
        )*
    };
}

reader_prefix_tests! {
    nthcdr_zero: "nthcdr-zero",
        no_prefix "(defun f (x) (nthcdr 0 nz))"
            => "(defun f (x) nz)",
        backquote "(defmacro m (x) `(nthcdr 0 nz))"
            => "(defmacro m (x) `nz)",
        unquote "(defmacro m (x) `(list ,(nthcdr 0 nz)))"
            => "(defmacro m (x) `(list ,nz))",
        splicing "(defmacro m (x) `(list ,@(nthcdr 0 nz)))"
            => "(defmacro m (x) `(list ,@nz))",
        sharp_quote "(defun f (x) (g #'(nthcdr 0 nz)))"
            => "(defun f (x) (g #'nz))",
        hard_quote "(defparameter *d* '(nthcdr 0 nz))"
            => Vec::<String>::new();
    car_nthcdr: "car-nthcdr",
        no_prefix "(defun f (x) (car (nthcdr 2 x)))"
            => "(defun f (x) (nth 2 x))",
        backquote "(defmacro m (x) `(car (nthcdr 2 x)))"
            => "(defmacro m (x) `(nth 2 x))",
        unquote "(defmacro m (x) `(list ,(car (nthcdr 2 x))))"
            => "(defmacro m (x) `(list ,(nth 2 x)))",
        splicing "(defmacro m (x) `(list ,@(car (nthcdr 2 x))))"
            => "(defmacro m (x) `(list ,@(nth 2 x)))",
        sharp_quote "(defun f (x) (g #'(car (nthcdr 2 x))))"
            => "(defun f (x) (g #'(nth 2 x)))",
        hard_quote "(defparameter *d* '(car (nthcdr 2 x)))"
            => vec!["(defparameter *d* '(nth 2 x))".to_owned()];
    append_list_to_cons: "append-list-to-cons",
        no_prefix "(defun f (x) (append (list al) ar))"
            => "(defun f (x) (cons al ar))",
        backquote "(defmacro m (x) `(append (list al) ar))"
            => "(defmacro m (x) `(cons al ar))",
        unquote "(defmacro m (x) `(list ,(append (list al) ar)))"
            => "(defmacro m (x) `(list ,(cons al ar)))",
        splicing "(defmacro m (x) `(list ,@(append (list al) ar)))"
            => "(defmacro m (x) `(list ,@(cons al ar)))",
        sharp_quote "(defun f (x) (g #'(append (list al) ar)))"
            => "(defun f (x) (g #'(cons al ar)))",
        hard_quote "(defparameter *d* '(append (list al) ar))"
            => Vec::<String>::new();
    car_reverse: "car-reverse",
        no_prefix "(defun f (x) (car (reverse crx)))"
            => "(defun f (x) (car (last crx)))",
        backquote "(defmacro m (x) `(car (reverse crx)))"
            => "(defmacro m (x) `(car (last crx)))",
        unquote "(defmacro m (x) `(list ,(car (reverse crx))))"
            => "(defmacro m (x) `(list ,(car (last crx))))",
        splicing "(defmacro m (x) `(list ,@(car (reverse crx))))"
            => "(defmacro m (x) `(list ,@(car (last crx))))",
        sharp_quote "(defun f (x) (g #'(car (reverse crx))))"
            => "(defun f (x) (g #'(car (last crx))))",
        hard_quote "(defparameter *d* '(car (reverse crx)))"
            => Vec::<String>::new();
    double_reverse: "double-reverse",
        no_prefix "(defun f (x) (reverse (reverse dr)))"
            => "(defun f (x) (copy-seq dr))",
        backquote "(defmacro m (x) `(reverse (reverse dr)))"
            => "(defmacro m (x) `(copy-seq dr))",
        unquote "(defmacro m (x) `(list ,(reverse (reverse dr))))"
            => "(defmacro m (x) `(list ,(copy-seq dr)))",
        splicing "(defmacro m (x) `(list ,@(reverse (reverse dr))))"
            => "(defmacro m (x) `(list ,@(copy-seq dr)))",
        sharp_quote "(defun f (x) (g #'(reverse (reverse dr))))"
            => "(defun f (x) (g #'(copy-seq dr)))",
        hard_quote "(defparameter *d* '(reverse (reverse dr)))"
            => Vec::<String>::new();
}

/// `nthcdr-zero` alone puts an *operand's own source* where the whole form
/// stood, rather than a freshly parenthesized `(head …)`. So it alone can be
/// asked to emit a leading splicing unquote, and re-spanning it to
/// `content_span` is not enough — the rewrite has to be declined.
///
/// The other four are unreachable this way by construction: each builds its
/// replacement as `format!("(… )")`, so whatever the operands carry ends up
/// *inside* parentheses where a splice is well-formed. The
/// `carries_a_splicing_unquote` rows above are the evidence for that, and they
/// are why this module is not a fifth entry in the table.
///
/// This is the same shape `coerce-to-t` has in `lint-form-shape`, and it
/// carries the same guard for the same reason.
mod nthcdr_zero_spliced_operand {
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
        &crate::nthcdr_zero::rule::META,
        &crate::nthcdr_zero::rule::RULE,
    )];

    fn fixed(source: &str) -> Vec<String> {
        run_rule_fixed(&ENTRIES, source)
            .into_iter()
            .map(|(_, fixed)| fixed)
            .collect()
    }

    /// `` `(nthcdr 0 ,@xs) `` re-spanned to `content_span` would produce
    /// `` `,@xs ``, which SBCL refuses to read: a splicing unquote has no list
    /// to splice into there. Confirmed against SBCL rather than assumed — it is
    /// one of the four files the read oracle lost under the pre-fix binary.
    ///
    /// The operand count is expansion-dependent besides — `xs` expanding to
    /// anything but one form means this was never the two-operand `nthcdr` the
    /// rule reasons about — so there is no rewrite to emit under either
    /// objection.
    #[test]
    fn a_spliced_operand_under_a_backquote_is_declined() {
        assert_eq!(
            fixed("(defmacro m (xs) `(nthcdr 0 ,@xs))"),
            Vec::<String>::new()
        );
    }

    /// `,.` splices exactly as `,@` does and `` ` `` rejects it identically
    /// (both verified against SBCL), but the parser labels it
    /// `ReaderPrefix::Unquote` — a separate defect recorded in PR #127. A guard
    /// written against the prefix enum rather than the replacement text would
    /// inherit that mislabelling and corrupt this source.
    #[test]
    fn an_unquote_dot_operand_under_a_backquote_is_declined() {
        assert_eq!(
            fixed("(defmacro m (xs) `(nthcdr 0 ,.xs))"),
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
            fixed("(defmacro m (x) `(nthcdr 0 ,x))"),
            vec!["(defmacro m (x) `,x)".to_owned()]
        );
    }

    /// The other half of that control: a splice *inside* the operand rather
    /// than leading it is well-formed once the operand keeps its parentheses,
    /// so the guard must not reach it.
    #[test]
    fn a_splice_nested_inside_the_operand_is_still_fixed() {
        assert_eq!(
            fixed("(defmacro m (xs) `(nthcdr 0 (g ,@xs)))"),
            vec!["(defmacro m (xs) `(g ,@xs))".to_owned()]
        );
    }
}
