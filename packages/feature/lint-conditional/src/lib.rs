#![doc = include_str!("../README.md")]

pub mod case_key_eql_pitfall;
pub mod case_nil_key;
pub mod cond_t_clause;
pub mod cond_to_case_candidate;
pub mod constant_if_test;
pub mod constant_when_test;
pub mod de_morgan;
pub mod dead_boolean_operand;
pub mod duplicate_boolean_operands;
pub mod duplicate_case_keys;
pub mod duplicate_cond_tests;
pub mod empty_body;
pub mod exhaustive_case_otherwise;
pub mod identical_if_branches;
pub mod if_arity;
pub mod if_not;
pub mod if_to_or;
pub mod if_to_unless;
pub mod malformed_case_clause;
pub mod malformed_cond_clause;
pub mod negated_comparison;
pub mod negated_if;
pub mod negated_when_unless;
pub mod nested_boolean;
pub mod nested_cond_flattenable;
pub mod nested_unless;
pub mod nested_when;
pub mod one_armed_if;
pub mod quoted_case_key;
pub mod redundant_boolean_identity;
pub mod redundant_if_nil;
pub mod single_clause_cond;
pub mod single_operand_boolean;
pub mod support;
pub mod typecase_nil_key;
pub mod unreachable_case_clause;
pub mod unreachable_cond_clause;
pub mod when_unless_implicit_nil_misused;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// The four rules added in this batch, driven through the *engine* rather than
/// through their own `build_*_report`.
///
/// The two entry points do not share their quote handling, so testing only the
/// report leaves half of each rule unexercised. A report walks with
/// [`crate::support::for_each_evaluated_subview`], which never visits data at
/// all; a head-filtered rule is handed matched nodes by the dispatcher
/// *including* the ones inside `'(…)`, and depends on each `check`'s
/// [`crate::support::is_unevaluated_at`] call to decline them.
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see — each rule's `HeadFilter` and its `RuleDialectScope`. A wrong `Heads`
/// list passes every `examine()` test while being unreachable from the CLI,
/// which is exactly what `every_rule_fires_through_the_real_dispatch` and
/// `no_rule_sees_a_file_without_any_of_their_heads` exist to catch.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(
            &crate::case_key_eql_pitfall::rule::META,
            &crate::case_key_eql_pitfall::rule::RULE,
        ),
        RuleEntry::new(
            &crate::cond_to_case_candidate::rule::META,
            &crate::cond_to_case_candidate::rule::RULE,
        ),
        RuleEntry::new(
            &crate::nested_cond_flattenable::rule::META,
            &crate::nested_cond_flattenable::rule::RULE,
        ),
        RuleEntry::new(
            &crate::when_unless_implicit_nil_misused::rule::META,
            &crate::when_unless_implicit_nil_misused::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order. Duplicates are kept: a rule that reports
    /// twice on one file appears twice.
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

    /// The distinct rule names that fire, for the sweeps where multiplicity is
    /// not the point.
    fn fired_once(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let mut names = fired(source, dialect);
        names.dedup();
        names
    }

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        assert_eq!(
            fired(
                "(cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c))",
                Dialect::CommonLisp
            ),
            vec!["cond-to-case-candidate"]
        );
        assert_eq!(
            fired(r#"(case c ("a" 1) (t 2))"#, Dialect::CommonLisp),
            vec!["case-key-eql-pitfall"]
        );
        assert_eq!(
            fired("(cond (a 1) (t (cond (b 2) (c 3))))", Dialect::CommonLisp),
            vec!["nested-cond-flattenable"]
        );
        assert_eq!(
            fired("(+ base (when p d))", Dialect::CommonLisp),
            vec!["when-unless-implicit-nil-misused"]
        );
    }

    /// `HeadFilter::Heads`: a file with none of the six anchoring heads is
    /// never handed to any of them, which is what keeps the zero-finding
    /// `clean/forms/*` benchmarks cheap.
    #[test]
    fn no_rule_sees_a_file_without_any_of_their_heads() {
        assert_eq!(
            fired(
                "(defun f (a b) (let ((x (list a b))) (mapcar #'identity x)))\n\
                 (defmethod frob ((x integer)) (values x x))\n\
                 (defmacro m (n) `(progn ,n))\n\
                 (if a b c)\n\
                 (and a b)\n\
                 (typecase x (string 1) (float 2))\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    // -- the guard the report path cannot exercise ---------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each `check`'s `is_unevaluated_at` call, every one of these
    /// fires.
    #[test]
    fn no_rule_fires_on_hard_quoted_data() {
        for source in [
            "'(cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c))",
            r#"'(case c ("a" 1) (t 2))"#,
            "'(cond (a 1) (t (cond (b 2) (c 3))))",
            "'(+ base (when p d))",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        for source in [
            "(quote (cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c)))",
            r#"(quote (case c ("a" 1) (t 2)))"#,
            "(quote (cond (a 1) (t (cond (b 2) (c 3)))))",
            "(quote (+ base (when p d)))",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    /// A macro template: the form is built, not run.
    #[test]
    fn no_rule_fires_inside_a_bare_quasiquoted_template() {
        assert_eq!(
            fired(
                "(defmacro m (a b c) `(cond (,a 1) (t (cond (,b 2) (,c 3)))))",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        for source in [
            "'(a ,(cond ((eql op 1) :a) ((eql op 2) :b) ((eql op 3) :c)))",
            r#"'(a ,(case c ("a" 1) (t 2)))"#,
            "'(a ,(cond (a 1) (t (cond (b 2) (c 3)))))",
            "'(a ,(+ base (when p d)))",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    /// The one shape that *is* code again.
    #[test]
    fn an_unquote_inside_a_quasiquote_still_fires() {
        assert_eq!(
            fired("`(a ,(+ base (when p d)))", Dialect::CommonLisp),
            vec!["when-unless-implicit-nil-misused"]
        );
        assert_eq!(
            fired(r#"`(a ,(case c ("a" 1) (t 2)))"#, Dialect::CommonLisp),
            vec!["case-key-eql-pitfall"]
        );
    }

    // -- the declarations a domain test cannot see ---------------------------

    /// `RuleDialectScope`: the dispatcher skips a rule before walking anything.
    #[test]
    fn no_rule_runs_outside_common_lisp() {
        for dialect in [
            Dialect::Clojure,
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::EmacsLisp,
            Dialect::Fennel,
        ] {
            assert_eq!(
                fired("(cond (a 1) (t (cond (b 2) (c 3))))", dialect),
                Vec::<&str>::new(),
                "{dialect:?}"
            );
        }
    }

    // -- realistic correct code ----------------------------------------------

    /// Hand-written Common Lisp that is *correct* and uses every construct the
    /// four rules anchor on, including the idioms each rule recommends. A
    /// reviewer runs this case first, and ~120 unit tests have missed real
    /// false positives that a file like this caught.
    const REALISTIC_CORRECT: &str = r#"
(in-package :app/tokens)

(defun classify (token)
  "Dispatch on a token's kind. A case with mixed literal key types is legal:
the token is a keyword, an integer, or a character."
  (case token
    (:eof :end)
    ((:lparen :rparen) :delimiter)
    (0 :zero)
    (#\a :letter-a)
    (100000000000000000000 :huge)
    (t :unknown)))

(defun describe-name (name)
  "String comparison belongs in cond with string=, never in case."
  (cond ((string= name "add") :add)
        ((string= name "sub") :sub)
        ((string= name "mul") :mul)
        (t :unknown)))

(defun price (base discount-p discount)
  "The or-guard and the two-armed if are the idioms that repair an
implicit-nil misuse; neither may be reported."
  (+ base
     (or (when discount-p discount) 0)
     (if discount-p discount 0)))

(defun tally (items)
  (let ((total 0))
    (dolist (item items total)
      (when (item-active-p item)
        (incf total (item-cost item))))))

(defun bucket (n)
  "Genuine nesting: the outer cond dispatches on sign and the inner on
magnitude. The final clause is not a bare t holding only a cond."
  (cond ((minusp n) :negative)
        ((zerop n) :zero)
        (t (if (> n 100) :large :small))))

(defun step-mode (mode)
  "A test-only clause returns the test's value; that idiom stays."
  (cond ((lookup-override mode))
        ((eql mode :fast) 1)
        (t 0)))

(defun near-p (x y)
  "Float comparison done properly, with a tolerance rather than a case key."
  (< (abs (- x y)) 1d-6))

(defmacro with-kind ((var form) &body body)
  "A quasiquoted template is data, not code."
  `(let ((,var (cond ((eql ,form 1) :a) ((eql ,form 2) :b) ((eql ,form 3) :c))))
     ,@body))
"#;

    #[test]
    fn a_realistic_correct_file_produces_no_findings() {
        assert_eq!(
            fired(REALISTIC_CORRECT, Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    /// The dangerous twin of the file above. Without this, "no findings on
    /// correct code" would be equally consistent with a harness that cannot
    /// detect anything at all.
    const REALISTIC_DANGEROUS: &str = r#"
(in-package :app/tokens)

(defun classify (op)
  (cond ((eql op 1) :add)
        ((eql op 2) :sub)
        ((eql op 3) :mul)
        (t :unknown)))

(defun describe-name (name)
  (case name
    ("add" :add)
    ("sub" :sub)
    (t :unknown)))

(defun price (base discount-p discount)
  (+ base (when discount-p discount)))

(defun bucket (n)
  (cond ((minusp n) :negative)
        (t (cond ((zerop n) :zero)
                 ((> n 100) :large)))))
"#;

    #[test]
    fn the_dangerous_twin_trips_every_one_of_the_four_rules() {
        assert_eq!(
            fired_once(REALISTIC_DANGEROUS, Dialect::CommonLisp),
            vec![
                "case-key-eql-pitfall",
                "cond-to-case-candidate",
                "nested-cond-flattenable",
                "when-unless-implicit-nil-misused",
            ]
        );
    }

    // -- the repository's own Lisp corpus ------------------------------------

    /// The repository's ordinary-code fixtures, included at compile time
    /// rather than read at run time: the nix sandbox has no git and a test
    /// that walks the tree would silently pass by finding no files.
    ///
    /// These are hand-written Common Lisp that nobody wrote to exercise these
    /// four rules, which is what makes them worth sweeping.
    const REPOSITORY_CORPUS: [(&str, &str); 5] = [
        (
            "tests/fixtures/corpus/clos.lisp",
            include_str!("../../../../tests/fixtures/corpus/clos.lisp"),
        ),
        (
            "tests/fixtures/corpus/deep-nesting.lisp",
            include_str!("../../../../tests/fixtures/corpus/deep-nesting.lisp"),
        ),
        (
            "tests/fixtures/corpus/reader-syntax.lisp",
            include_str!("../../../../tests/fixtures/corpus/reader-syntax.lisp"),
        ),
        (
            "tests/fixtures/semantic_coverage_corpus/utilities.lisp",
            include_str!("../../../../tests/fixtures/semantic_coverage_corpus/utilities.lisp"),
        ),
        (
            "tests/fixtures/semantic_coverage_corpus/geometry.lisp",
            include_str!("../../../../tests/fixtures/semantic_coverage_corpus/geometry.lisp"),
        ),
    ];

    #[test]
    fn the_repository_corpus_produces_no_findings() {
        for (name, source) in REPOSITORY_CORPUS {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{name} is ordinary correct Common Lisp"
            );
        }
    }

    /// The corpus sweep above proves nothing on its own unless the harness
    /// that runs it can still see a defect, so it is run once more with a
    /// known-bad form appended to every fixture.
    #[test]
    fn the_corpus_harness_still_detects_an_appended_defect() {
        for (name, source) in REPOSITORY_CORPUS {
            let spiked = format!("{source}\n(+ base (when p d))\n");
            assert_eq!(
                fired(&spiked, Dialect::CommonLisp),
                vec!["when-unless-implicit-nil-misused"],
                "{name} with a defect appended"
            );
        }
    }
}
