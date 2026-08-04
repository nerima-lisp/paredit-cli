//! The hard-quote guard, per guarded rule, through the real dispatch.
//!
//! Every `Fixability::Fixable` rule in this package rewrites source, and the
//! lint engine's dispatch walks into quoted data like any other subtree. A rule
//! that fires inside `'(…)` therefore rewrites a *data literal* as if it were
//! code, and `--fix` silently corrupts the file — which no round-trip property
//! catches, because a consistently wrong parse is a fixed point. Each rule now
//! drops such a finding instead of offering a fix for it.
//!
//! The verdict is read on the `hard` counter alone and never on
//! [`crate::support::QuoteState::is_data`]. The two tests per rule are what pin
//! that distinction down:
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
//! ancestor and not a node-local `reader_prefixes` check.
//!
//! `sharp-quoted-lambda` is deliberately not guarded and so is absent here: over
//! the 27,880-file corpus this guard was measured on, 10 of its 14 hard-quoted
//! findings are live code — 8 spliced back as source by `#.`, 2 reached through
//! a `',(…)` unquote — so a guard would cost more correct fixes than it would
//! prevent corrupting ones.

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
    butlast_default_count: "butlast-default-count",
        template "(defmacro m () `(progn (butlast x 1)))",
        becomes "(defmacro m () `(progn (butlast x)))",
        inert "(defparameter *d* '(progn (butlast x 1)))";
    coerce_to_t: "coerce-to-t",
        template "(defmacro m () `(progn (coerce x t)))",
        becomes "(defmacro m () `(progn x))",
        inert "(defparameter *d* '(progn (coerce x t)))";
    empty_let: "empty-let",
        template "(defmacro m () `(progn (let () x)))",
        becomes "(defmacro m () `(progn (progn x)))",
        inert "(defparameter *d* '(progn (let () x)))";
    funcall_lambda: "funcall-lambda",
        template "(defmacro m () `(progn (funcall (lambda (y) y) x)))",
        becomes "(defmacro m () `(progn ((lambda (y) y) x)))",
        inert "(defparameter *d* '(progn (funcall (lambda (y) y) x)))";
    getf_default_nil: "getf-default-nil",
        template "(defmacro m () `(progn (getf p k nil)))",
        becomes "(defmacro m () `(progn (getf p k)))",
        inert "(defparameter *d* '(progn (getf p k nil)))";
    gethash_default: "gethash-default",
        template "(defmacro m () `(progn (gethash k h nil)))",
        becomes "(defmacro m () `(progn (gethash k h)))",
        inert "(defparameter *d* '(progn (gethash k h nil)))";
    make_array_default_keyword: "make-array-default-keyword",
        template "(defmacro m () `(progn (make-array n :adjustable nil)))",
        becomes "(defmacro m () `(progn (make-array n)))",
        inert "(defparameter *d* '(progn (make-array n :adjustable nil)))";
    make_hash_table_test: "make-hash-table-test",
        template "(defmacro m () `(progn (make-hash-table :test 'eql)))",
        becomes "(defmacro m () `(progn (make-hash-table)))",
        inert "(defparameter *d* '(progn (make-hash-table :test 'eql)))";
    make_list_default_element: "make-list-default-element",
        template "(defmacro m () `(progn (make-list n :initial-element nil)))",
        becomes "(defmacro m () `(progn (make-list n)))",
        inert "(defparameter *d* '(progn (make-list n :initial-element nil)))";
    manual_incf: "manual-incf",
        template "(defmacro m () `(progn (setf x (1+ x))))",
        becomes "(defmacro m () `(progn (incf x)))",
        inert "(defparameter *d* '(progn (setf x (1+ x))))";
    manual_push: "manual-push",
        template "(defmacro m () `(progn (setf l (cons v l))))",
        becomes "(defmacro m () `(progn (push v l)))",
        inert "(defparameter *d* '(progn (setf l (cons v l))))";
    manual_pushnew: "manual-pushnew",
        template "(defmacro m () `(progn (setf l (adjoin v l))))",
        becomes "(defmacro m () `(progn (pushnew v l)))",
        inert "(defparameter *d* '(progn (setf l (adjoin v l))))";
    multiple_value_list_of_values: "multiple-value-list-of-values",
        template "(defmacro m () `(progn (multiple-value-list (values a b))))",
        becomes "(defmacro m () `(progn (list a b)))",
        inert "(defparameter *d* '(progn (multiple-value-list (values a b))))";
    nested_char_case: "nested-char-case",
        template "(defmacro m () `(progn (char-upcase (char-downcase c))))",
        becomes "(defmacro m () `(progn (char-upcase c)))",
        inert "(defparameter *d* '(progn (char-upcase (char-downcase c))))";
    nested_cxr: "nested-cxr",
        template "(defmacro m () `(progn (car (cdr x))))",
        becomes "(defmacro m () `(progn (cadr x)))",
        inert "(defparameter *d* '(progn (car (cdr x))))";
    parse_integer_default_radix: "parse-integer-default-radix",
        template "(defmacro m () `(progn (parse-integer s :radix 10)))",
        becomes "(defmacro m () `(progn (parse-integer s)))",
        inert "(defparameter *d* '(progn (parse-integer s :radix 10)))";
    redundant_apply: "redundant-apply",
        template "(defmacro m () `(progn (apply #'g (list a b))))",
        becomes "(defmacro m () `(progn (g a b)))",
        inert "(defparameter *d* '(progn (apply #'g (list a b))))";
    redundant_funcall: "redundant-funcall",
        template "(defmacro m () `(progn (funcall #'g x)))",
        becomes "(defmacro m () `(progn (g x)))",
        inert "(defparameter *d* '(progn (funcall #'g x)))";
    redundant_identity: "redundant-identity",
        template "(defmacro m () `(progn (identity x)))",
        becomes "(defmacro m () `(progn x))",
        inert "(defparameter *d* '(progn (identity x)))";
    redundant_let_star: "redundant-let-star",
        template "(defmacro m () `(progn (let* ((a v)) b)))",
        becomes "(defmacro m () `(progn (let ((a v)) b)))",
        inert "(defparameter *d* '(progn (let* ((a v)) b)))";
    redundant_quote: "redundant-quote",
        template "(defmacro m () `(progn (f '5)))",
        becomes "(defmacro m () `(progn (f 5)))",
        inert "(defparameter *d* '(progn (f '5)))";
    redundant_the: "redundant-the",
        template "(defmacro m () `(progn (the t x)))",
        becomes "(defmacro m () `(progn x))",
        inert "(defparameter *d* '(progn (the t x)))";
    single_value_bind: "single-value-bind",
        template "(defmacro m () `(progn (multiple-value-bind (x) f b)))",
        becomes "(defmacro m () `(progn (let ((x f)) b)))",
        inert "(defparameter *d* '(progn (multiple-value-bind (x) f b)))";
    typep_predicate: "typep-predicate",
        template "(defmacro m () `(progn (typep x 'string)))",
        becomes "(defmacro m () `(progn (stringp x)))",
        inert "(defparameter *d* '(progn (typep x 'string)))";
    values_list_of_list: "values-list-of-list",
        template "(defmacro m () `(progn (values-list (list a b))))",
        becomes "(defmacro m () `(progn (values a b)))",
        inert "(defparameter *d* '(progn (values-list (list a b))))";
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

    static CXR: [RuleEntry; 1] = [RuleEntry::new(
        &crate::nested_cxr::rule::META,
        &crate::nested_cxr::rule::RULE,
    )];

    static QUOTE: [RuleEntry; 1] = [RuleEntry::new(
        &crate::redundant_quote::rule::META,
        &crate::redundant_quote::rule::RULE,
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
            fixed(&CXR, "(defparameter *d* '(car (cdr x)))"),
            Vec::<String>::new()
        );
    }

    /// The same datum spelled long-hand. A guard that only understood the `'`
    /// reader prefix would rewrite this one.
    #[test]
    fn a_long_hand_quote_form_is_inert() {
        assert_eq!(
            fixed(&CXR, "(defparameter *d* (quote (progn (car (cdr x)))))"),
            Vec::<String>::new()
        );
    }

    /// The control for both tests above: with no quote anywhere, the rule
    /// still fixes. Without this, a guard that suppressed everything would
    /// pass the two of them.
    #[test]
    fn the_same_form_is_still_fixed_as_plain_code() {
        assert_eq!(
            fixed(&CXR, "(defun f (x) (car (cdr x)))"),
            vec!["(defun f (x) (cadr x))".to_owned()]
        );
    }

    /// `redundant-quote` reads the *enclosing* state, so its own `'` must not
    /// answer the question: this quote is redundant sugar and is removed.
    #[test]
    fn a_redundant_quote_in_code_is_still_fixed() {
        assert_eq!(
            fixed(&QUOTE, "(defun f () (list '5))"),
            vec!["(defun f () (list 5))".to_owned()]
        );
    }

    /// The long-hand spelling of the same, which is what the rule's other
    /// half reports.
    #[test]
    fn a_long_hand_redundant_quote_in_code_is_still_fixed() {
        assert_eq!(
            fixed(&QUOTE, "(defun f () (quote 5))"),
            vec!["(defun f () 5)".to_owned()]
        );
    }

    /// And the enclosing quote it does have to respect, spelled long-hand.
    #[test]
    fn a_redundant_quote_inside_a_long_hand_quote_is_inert() {
        assert_eq!(
            fixed(&QUOTE, "(defparameter *d* (quote (progn (f '5))))"),
            Vec::<String>::new()
        );
    }
}
