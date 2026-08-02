#![doc = include_str!("../README.md")]

pub mod char_case_fold;
pub mod char_op_string;
pub mod code_char_char_code;
pub mod format_missing_destination;
pub mod format_nested_directive_unbalanced;
pub mod format_newline;
pub mod format_percent_ampersand_adjacent_redundancy;
pub mod format_to_string;
pub mod format_unknown_directive;
pub mod nested_string_case;
pub mod string_case_fold;
pub mod support;

// The composition root's REGISTRY names each rule's META and RULE across the
// crate boundary; the inspect subcommands are reached through each slice's cli.

/// The three control-string rules driven through the *engine*, rather than
/// through their own `build_*_report`.
///
/// The two entry points do not share their quote handling. A report walks with
/// [`crate::support::for_each_evaluated_subview`], which never visits data at
/// all; a head-filtered rule is handed matched nodes by the dispatcher
/// *including* the ones inside `'(…)`, and depends on each `check`'s
/// [`crate::support::is_unevaluated_at`] call to decline them. Testing only the
/// reports would leave that call unexercised.
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see: each rule's `HeadFilter::Heads` and its `RuleDialectScope`.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// The three new rules, plus the three `format` rules that already shipped.
    ///
    /// The incumbents are here on purpose: the claim that the new triggers are
    /// disjoint from `format-newline`, `format-missing-destination` and
    /// `format-to-string` is only worth anything if all six run over the same
    /// source and the assertion names exactly who fired.
    static ENTRIES: [RuleEntry; 6] = [
        RuleEntry::new(
            &crate::format_missing_destination::rule::META,
            &crate::format_missing_destination::rule::RULE,
        ),
        RuleEntry::new(
            &crate::format_nested_directive_unbalanced::rule::META,
            &crate::format_nested_directive_unbalanced::rule::RULE,
        ),
        RuleEntry::new(
            &crate::format_newline::rule::META,
            &crate::format_newline::rule::RULE,
        ),
        RuleEntry::new(
            &crate::format_percent_ampersand_adjacent_redundancy::rule::META,
            &crate::format_percent_ampersand_adjacent_redundancy::rule::RULE,
        ),
        RuleEntry::new(
            &crate::format_to_string::rule::META,
            &crate::format_to_string::rule::RULE,
        ),
        RuleEntry::new(
            &crate::format_unknown_directive::rule::META,
            &crate::format_unknown_directive::rule::RULE,
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

    fn cl(source: &str) -> Vec<&'static str> {
        fired(source, Dialect::CommonLisp)
    }

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_new_rule_fires_through_the_real_dispatch() {
        assert_eq!(cl("(format t \"~Q\" x)"), vec!["format-unknown-directive"]);
        assert_eq!(
            cl("(format t \"a~%~&b\")"),
            vec!["format-percent-ampersand-adjacent-redundancy"]
        );
        assert_eq!(
            cl("(format t \"~{~a\" xs)"),
            vec!["format-nested-directive-unbalanced"]
        );
    }

    /// The four non-`format` heads reach the dispatcher through the same head
    /// filter, which a `format`-only test would not prove.
    #[test]
    fn the_other_four_format_family_heads_reach_the_dispatch() {
        assert_eq!(cl("(error \"~Q\" x)"), vec!["format-unknown-directive"]);
        assert_eq!(cl("(warn \"~Q\" x)"), vec!["format-unknown-directive"]);
        assert_eq!(
            cl("(cerror \"Retry.\" \"~Q\" x)"),
            vec!["format-unknown-directive"]
        );
        assert_eq!(
            cl("(format-to-string \"~Q\" x)"),
            vec!["format-unknown-directive"]
        );
    }

    // -- the guard the report path cannot exercise ---------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each `check`'s `is_unevaluated_at` call, every one of these
    /// fires.
    #[test]
    fn no_new_rule_fires_on_the_four_data_quote_shapes() {
        for source in [
            "'(format t \"~Q~{~a\")",
            "(quote (format t \"~Q~{~a\"))",
            "`(format t \"~Q~{~a\")",
            "'(a ,(format t \"~Q~{~a\"))",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is data"
            );
        }
        assert_eq!(
            cl("'(format t \"a~%~&b\")"),
            Vec::<&str>::new(),
            "a hard-quoted call is data"
        );
    }

    /// A macro template: the `format` call is built, not run.
    #[test]
    fn no_new_rule_fires_inside_a_quasiquoted_macro_template() {
        assert_eq!(
            cl("(defmacro trace-it (x) `(format t \"~Q\" ,x))"),
            Vec::<&str>::new()
        );
    }

    /// The one shape that *is* code again — and the shape a single `i32` depth
    /// counter gets wrong in the other direction.
    #[test]
    fn an_unquote_inside_a_quasiquote_still_fires() {
        assert_eq!(
            cl("`(a ,(format t \"~Q\" x))"),
            vec!["format-unknown-directive"]
        );
    }

    // -- the declarations a domain test cannot see ---------------------------

    /// `RuleDialectScope`: the dispatcher skips a rule before walking anything.
    /// `format` exists in Emacs Lisp with `%` directives, so a rule reading
    /// `~` sequences there would be reading ordinary text.
    #[test]
    fn no_new_rule_runs_outside_common_lisp() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Clojure,
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Fennel,
        ] {
            assert_eq!(
                fired("(format t \"~Q~{~a\" x)", dialect),
                Vec::<&str>::new(),
                "{dialect:?} is not modelled"
            );
        }
    }

    /// `HeadFilter::Heads`: an ordinary definition is never handed to any of
    /// these rules, which is what keeps the zero-finding benchmarks cheap.
    #[test]
    fn no_rule_sees_a_form_that_is_not_a_format_family_call() {
        assert_eq!(
            cl("(defun f (a b) (+ a b))\n(list \"~Q~{~a\" 1)\n(princ \"~Q\")\n"),
            Vec::<&str>::new()
        );
    }

    // -- the disjointness claims --------------------------------------------

    /// `format-newline`'s control string is exactly `~%`, which has no `&` and
    /// no bracket and no unknown directive in it.
    #[test]
    fn format_newline_fires_alone_on_its_own_subject() {
        assert_eq!(cl("(format t \"~%\")"), vec!["format-newline"]);
    }

    /// `format-missing-destination`'s subject puts the literal in the
    /// *destination* slot, so the control slot holds a variable and none of the
    /// three new rules has a literal to read.
    #[test]
    fn format_missing_destination_fires_alone_on_its_own_subject() {
        assert_eq!(
            cl("(format \"~Q~{~a\" x)"),
            vec!["format-missing-destination"]
        );
    }

    /// `format-to-string`'s subject is a well-formed one-directive string.
    #[test]
    fn format_to_string_fires_alone_on_its_own_subject() {
        assert_eq!(cl("(format nil \"~A\" x)"), vec!["format-to-string"]);
    }

    /// Two independent defects in one string produce two findings, which is
    /// the other half of "disjoint": neither rule suppresses the other.
    #[test]
    fn two_defects_in_one_control_string_are_two_findings() {
        assert_eq!(
            cl("(format t \"~Q~%~&\" x)"),
            vec![
                "format-percent-ampersand-adjacent-redundancy",
                "format-unknown-directive"
            ]
        );
    }

    // -- the corpus sweep ----------------------------------------------------

    /// A realistic, correct Common Lisp file that leans hard on `format`: the
    /// case a reviewer runs first.
    ///
    /// Every construct here is either idiomatic CLHS-conforming usage or one of
    /// the specific traps these rules are built to survive — the literal tilde,
    /// signed and quoted prefix parameters, `~/name/`, the ignored newline,
    /// `~&~%`, and a `~?` whose inner control string is an *argument*.
    const CORRECT_CORPUS: &str = r#"(in-package :app/report)

(defun banner (stream)
  (format stream "~&~%== Report ==~%")
  (format stream "~V,,,'-<~>~%" 40)
  (format t "100~~ complete~%"))

(defun render-row (stream name count ratio)
  (format stream "~20a ~5,'0d ~,2f~%" name count ratio)
  (format stream "~a~30t~a~%" name count)
  (format stream "~3,-4:@s / ~,+4s~%" name count))

(defun render-all (stream rows)
  (format stream "~{~a~^, ~}~%" rows)
  (format stream "~:[none~;~:*~d found~]~%" (length rows))
  (format stream "~#[nothing~;one thing~:;~:*~d things~]~%" (length rows))
  (format stream "~<~a~;~a~:>~%" (list "left" "right"))
  (format stream "~(~a~) and ~:@(~a~)~%" "SHOUT" "quiet")
  (format stream "~/app-report:print-cell/~%" (first rows))
  (format stream "a very long line that continues ~
                  onto the next source line: ~a~%" rows))

(defun explain (stream template arguments)
  (format stream "~&Explanation:~%")
  (format stream "~?~%" template arguments)
  (format stream "~a~%" "~Q is printed literally, not interpreted")
  (format stream "brackets in text: [~a] {~a} (~a) <~a>~%" 1 2 3 4))

(defun complain (name)
  (error "no such thing: ~s~%" name)
  (warn "~a is deprecated; use ~a instead~%" name 'other)
  (cerror "Skip it." "cannot read ~a~%" name))
"#;

    #[test]
    fn a_correct_format_heavy_file_produces_no_findings() {
        assert_eq!(cl(CORRECT_CORPUS), Vec::<&str>::new());
    }

    /// The corpus is only worth anything if it actually *reaches* each rule.
    /// Counting candidates rather than findings is the point: a corpus with no
    /// literal control strings in it would pass the test above while proving
    /// nothing at all.
    #[test]
    fn the_correct_corpus_exercises_every_rule_it_is_meant_to_clear() {
        use crate::support::{directives, literal_control_string};
        use paredit_core_syntax::view_query::for_each_subview;

        let tree = SyntaxTree::parse_with_dialect(CORRECT_CORPUS, Dialect::CommonLisp)
            .expect("parse the corpus");

        let mut control_strings = 0;
        // Candidates for `format-unknown-directive`: any directive at all.
        let mut directive_count = 0;
        // Candidates for `format-percent-ampersand-adjacent-redundancy`: a `~&`
        // anywhere, which is what its cheap disqualifier looks for.
        let mut fresh_line_strings = 0;
        // Candidates for `format-nested-directive-unbalanced`: a bracketing
        // directive that the balance check has to match up.
        let mut bracket_directives = 0;

        for_each_subview(&tree.root_view(), |view| {
            let Some(raw) = literal_control_string(view) else {
                return;
            };
            control_strings += 1;
            let mut has_fresh_line = false;
            for directive in directives(raw) {
                directive_count += 1;
                if directive.character == '&' {
                    has_fresh_line = true;
                }
                if matches!(
                    directive.folded(),
                    '[' | ']' | '{' | '}' | '<' | '>' | '(' | ')'
                ) {
                    bracket_directives += 1;
                }
            }
            if has_fresh_line {
                fresh_line_strings += 1;
            }
        });

        assert!(
            control_strings >= 15,
            "the corpus must contain real control strings, found {control_strings}"
        );
        assert!(
            directive_count >= 50,
            "the corpus must contain real directives, found {directive_count}"
        );
        assert!(
            fresh_line_strings >= 2,
            "the corpus must reach the ~& rule's disqualifier, found {fresh_line_strings}"
        );
        assert!(
            bracket_directives >= 12,
            "the corpus must exercise the balance check, found {bracket_directives}"
        );
    }

    /// The dangerous twin of the corpus above: the same file with one defect
    /// introduced per rule, so a rule that has quietly stopped detecting
    /// anything cannot pass by reporting nothing.
    #[test]
    fn the_dangerous_twin_of_the_corpus_is_caught() {
        let twin = CORRECT_CORPUS
            // `~Q` is not a directive CLHS defines.
            .replace(
                r#"(format stream "~a~30t~a~%" name count)"#,
                r#"(format stream "~a~30t~Q~%" name count)"#,
            )
            // `~&` directly after `~%`, rather than before it.
            .replace(r#""~&~%== Report ==~%""#, r#""~%~&== Report ==~%""#)
            // An iteration opened and never closed.
            .replace(r#""~{~a~^, ~}~%""#, r#""~{~a~^, ~%""#);
        assert_ne!(twin, CORRECT_CORPUS, "the twin must actually differ");
        assert_eq!(
            fired(&twin, Dialect::CommonLisp),
            vec![
                "format-nested-directive-unbalanced",
                "format-percent-ampersand-adjacent-redundancy",
                "format-unknown-directive"
            ]
        );
    }
}
