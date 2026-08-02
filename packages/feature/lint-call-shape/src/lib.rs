#![doc = include_str!("../README.md")]

pub mod deeply_nested_anonymous_lambda;
pub mod nested_function_parameter_shadows_enclosing_parameter;
pub mod overly_long_parameter_list;
pub mod positional_argument_count_exceeds_readability;
pub mod stringly_typed_dispatch;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// The two entry points do not share their quote handling, and a rule is
/// reachable from the CLI only through this one. A domain test builds the
/// report, whose walk starts at each root child and asks `examine_*` about
/// every node; the dispatcher instead consults a *head index* built from each
/// rule's `HeadFilter`, and hands over only the nodes that match — including
/// the ones inside `'(…)`, which every `check` here has to decline for itself.
///
/// So a `Heads` list with a typo, a head a rule reads but never declared, and a
/// `dialect_scope` that is wrong in either direction all pass every `examine()`
/// test in this package while making the rule unreachable, or reachable where
/// it should not be. Those three declarations are what this module exercises.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 5] = [
        RuleEntry::new(
            &crate::deeply_nested_anonymous_lambda::rule::META,
            &crate::deeply_nested_anonymous_lambda::rule::RULE,
        ),
        RuleEntry::new(
            &crate::nested_function_parameter_shadows_enclosing_parameter::rule::META,
            &crate::nested_function_parameter_shadows_enclosing_parameter::rule::RULE,
        ),
        RuleEntry::new(
            &crate::overly_long_parameter_list::rule::META,
            &crate::overly_long_parameter_list::rule::RULE,
        ),
        RuleEntry::new(
            &crate::positional_argument_count_exceeds_readability::rule::META,
            &crate::positional_argument_count_exceeds_readability::rule::RULE,
        ),
        RuleEntry::new(
            &crate::stringly_typed_dispatch::rule::META,
            &crate::stringly_typed_dispatch::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        names.sort_unstable();
        names
    }

    // One source per rule, each chosen so that the *other* four say nothing
    // about it — otherwise a rule reachable only by accident would still look
    // reachable.
    const DEEP_LAMBDA: &str = "(lambda (a) (lambda (b) (lambda (c) c)))";
    const LONG_LAMBDA_LIST: &str = "(defun render (a b c d e f g h) nil)";
    const STRING_DISPATCH: &str = "(cond ((string= mode \"read\") 1) ((string= mode \"write\") 2) ((string= mode \"append\") 3) ((string= mode \"update\") 4))";
    const LITERAL_CALL: &str = "(defun f () (emit 1 2 3 \"a\" nil))";
    const SHADOWED_PARAMETER: &str =
        "(defun draw (window) (flet ((paint (window) window)) (paint (main))))";

    const EVERY_RULE: [(&str, &str); 5] = [
        (DEEP_LAMBDA, "deeply-nested-anonymous-lambda"),
        (LONG_LAMBDA_LIST, "overly-long-parameter-list"),
        (STRING_DISPATCH, "stringly-typed-dispatch"),
        (
            LITERAL_CALL,
            "positional-argument-count-exceeds-readability",
        ),
        (
            SHADOWED_PARAMETER,
            "nested-function-parameter-shadows-enclosing-parameter",
        ),
    ];

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        for (source, rule) in EVERY_RULE {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                vec![rule],
                "{rule} must be reachable through the head index, and alone"
            );
        }
    }

    // -- the guard the report path cannot exercise ---------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each `check`'s own quote verdict, every one of these fires.
    #[test]
    fn no_rule_fires_on_hard_quoted_data() {
        for (source, rule) in EVERY_RULE {
            assert_eq!(
                fired(&format!("'{source}"), Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{rule}: '{source} is data"
            );
        }
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        for (source, rule) in EVERY_RULE {
            assert_eq!(
                fired(&format!("(quote {source})"), Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{rule}: (quote …) is data"
            );
        }
    }

    /// A macro template: the form is built, not evaluated.
    #[test]
    fn no_rule_fires_inside_a_quasiquoted_macro_template() {
        for (source, rule) in EVERY_RULE {
            assert_eq!(
                fired(
                    &format!("(defmacro m (n) `(progn {source}))"),
                    Dialect::CommonLisp
                ),
                Vec::<&str>::new(),
                "{rule}: a quasiquoted template is data"
            );
        }
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        for (source, rule) in EVERY_RULE {
            assert_eq!(
                fired(&format!("'(x ,{source})"), Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{rule}: a comma inside '(…) is a comma"
            );
        }
    }

    /// The one shape that *is* code again.
    #[test]
    fn an_unquoted_form_inside_a_quasiquote_still_fires() {
        for (source, rule) in EVERY_RULE {
            assert_eq!(
                fired(&format!("`(x ,{source})"), Dialect::CommonLisp),
                vec![rule],
                "{rule}: an unquote is code"
            );
        }
    }

    /// A string literal is one atom, so nothing inside one is ever a form.
    #[test]
    fn no_rule_fires_on_a_form_spelled_only_inside_a_string() {
        for (source, rule) in EVERY_RULE {
            let escaped = source.replace('\\', "\\\\").replace('"', "\\\"");
            assert_eq!(
                fired(
                    &format!("(defvar *doc* \"{escaped}\")"),
                    Dialect::CommonLisp
                ),
                Vec::<&str>::new(),
                "{rule}: a string is one atom"
            );
        }
    }

    // -- the declarations a domain test cannot see ---------------------------

    /// `RuleDialectScope`: the dispatcher skips a rule before walking anything.
    #[test]
    fn no_rule_runs_outside_its_declared_dialects() {
        for dialect in [
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Fennel,
        ] {
            for (source, rule) in EVERY_RULE {
                assert_eq!(
                    fired(source, dialect),
                    Vec::<&str>::new(),
                    "{rule} must not run in {dialect:?}"
                );
            }
        }
    }

    /// The two rules that declare Emacs Lisp run there, and the three that do
    /// not, do not.
    #[test]
    fn emacs_lisp_runs_exactly_the_two_rules_that_declare_it() {
        assert_eq!(
            fired(DEEP_LAMBDA, Dialect::EmacsLisp),
            vec!["deeply-nested-anonymous-lambda"]
        );
        assert_eq!(
            fired(STRING_DISPATCH, Dialect::EmacsLisp),
            vec!["stringly-typed-dispatch"]
        );
        assert_eq!(
            fired(LONG_LAMBDA_LIST, Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
        assert_eq!(fired(LITERAL_CALL, Dialect::EmacsLisp), Vec::<&str>::new());
        assert_eq!(
            fired(SHADOWED_PARAMETER, Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
    }

    /// `HeadFilter::Heads`: a file with none of the declared heads reaches no
    /// rule at all, which is what keeps the zero-finding benchmarks cheap.
    #[test]
    fn a_file_with_none_of_the_declared_heads_trips_nothing() {
        let source = "(defpackage :app (:use :cl))\n(in-package :app)\n\
             (defvar *limit* 10)\n(defparameter *name* \"app\")\n\
             (defclass widget () ((size :initarg :size)))\n\
             (defstruct point x y)\n\
             (let ((a 1) (b 2)) (setq a (+ a b)))\n\
             (deftype small-int () '(integer 0 9))\n";
        assert_eq!(fired(source, Dialect::CommonLisp), Vec::<&str>::new());
    }

    /// A realistic, correct Common Lisp file: the case a reviewer runs first.
    #[test]
    fn a_correct_file_produces_no_findings() {
        let source = "(defpackage :app (:use :cl) (:export #:render #:flatten))\n(in-package :app)\n\n\
             (defun flatten (tree)\n  (labels ((walk (tree acc)\n             (cond ((null tree) acc)\n                   ((atom tree) (cons tree acc))\n                   (t (walk (car tree) (walk (cdr tree) acc))))))\n    (walk tree nil)))\n\n\
             (defun make-window (title width height &key resizable fullscreen decorated)\n  (declare (ignore resizable fullscreen decorated))\n  (list title width height))\n\n\
             (defmethod render ((w window) canvas)\n  (fill-rectangle canvas (window-x w) (window-y w) (window-width w) (window-height w))\n  (mapcar (lambda (child) (render child canvas)) (window-children w)))\n\n\
             (defun make-adder (n)\n  (lambda (x) (+ x n)))\n\n\
             (defun describe-mode (mode)\n  (case mode\n    (:read \"reading\")\n    (:write \"writing\")\n    (t \"unknown\")))\n\n\
             (defun palette ()\n  (list (rgb 255 0 0) (rgb 0 255 0) (rgb 0 0 255)))\n\n\
             (defmacro with-window ((var title) &body body)\n  `(let ((,var (make-window ,title 800 600)))\n     ,@body))\n";
        assert_eq!(fired(source, Dialect::CommonLisp), Vec::<&str>::new());
    }

    /// A realistic, correct Emacs Lisp file, for the two rules that claim it.
    #[test]
    fn a_correct_emacs_lisp_file_produces_no_findings() {
        let source = "(require 'cl-lib)\n\n\
             (defun app-visible-buffers ()\n  (seq-filter (lambda (buffer) (buffer-live-p buffer)) (buffer-list)))\n\n\
             (defun app-classify (name)\n  (cond ((string= name \"*scratch*\") 'scratch)\n        ((string-prefix-p \" \" name) 'internal)\n        (t 'normal)))\n\n\
             (defun app-make-counter (start)\n  (lambda () (setq start (1+ start))))\n";
        assert_eq!(fired(source, Dialect::EmacsLisp), Vec::<&str>::new());
    }
}
