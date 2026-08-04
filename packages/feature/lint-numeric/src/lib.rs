#![doc = include_str!("../README.md")]

pub mod division_result_precision_loss;
pub mod epsilon_less_float_loop_bound;
pub mod eq_char_comparison;
pub mod eq_number_comparison;
pub mod eql_list_comparison;
pub mod eql_string_comparison;
pub mod equality_arity;
pub mod explicit_step_delta;
pub mod identity_arithmetic;
pub mod literal_place;
pub mod mixed_float_precision_arithmetic;
pub mod modify_macro_arity;
pub mod negated_step_delta;
pub mod nil_comparison;
pub mod one_step_arithmetic;

#[cfg(test)]
mod quote_guard_tests;
pub mod redundant_divisor;
pub mod redundant_precision_coercion;
pub mod self_comparison;
pub mod sign_comparison;
pub mod single_arg_comparison;
pub mod single_operand_arithmetic;
pub mod step_zero;
pub mod support;
pub mod t_comparison;
pub mod verbose_negation;
pub mod zero_divisor;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// The four numeric-precision rules, driven through the *real* engine rather
/// than by calling `examine` directly.
///
/// Everything a unit test on `examine` cannot see lives here: the head index,
/// the dialect scope filter, and — the reason this module exists at all — the
/// fact that the dispatcher hands a rule every head-matched node whether or not
/// it is evaluated code. Without each `check`'s `is_unevaluated_at` call, every
/// quoted case below fires.
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
            &crate::division_result_precision_loss::rule::META,
            &crate::division_result_precision_loss::rule::RULE,
        ),
        RuleEntry::new(
            &crate::epsilon_less_float_loop_bound::rule::META,
            &crate::epsilon_less_float_loop_bound::rule::RULE,
        ),
        RuleEntry::new(
            &crate::mixed_float_precision_arithmetic::rule::META,
            &crate::mixed_float_precision_arithmetic::rule::RULE,
        ),
        RuleEntry::new(
            &crate::redundant_precision_coercion::rule::META,
            &crate::redundant_precision_coercion::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    pub(crate) fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let path = if dialect == Dialect::EmacsLisp {
            Path::new("t.el")
        } else {
            Path::new("t.lisp")
        };
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            path,
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

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        assert_eq!(
            fired("(* 3.14 1.0d0)", Dialect::CommonLisp),
            vec!["mixed-float-precision-arithmetic"]
        );
        assert_eq!(
            fired(
                "(do ((x 0.0 (+ x 0.1))) ((= x 1)) (body))",
                Dialect::CommonLisp
            ),
            vec!["epsilon-less-float-loop-bound"]
        );
        assert_eq!(
            fired("(truncate (coerce x 'double-float))", Dialect::CommonLisp),
            vec!["redundant-precision-coercion"]
        );
        assert_eq!(
            fired("(/ 1 3)", Dialect::EmacsLisp),
            vec!["division-result-precision-loss"]
        );
    }

    // -- the dialect scope the report path cannot exercise -------------------

    /// `(/ 1 3)` is the exact ratio 1/3 in Common Lisp. The dispatcher must drop
    /// the rule before the walk, not report and then filter.
    #[test]
    fn the_emacs_lisp_rule_never_fires_on_common_lisp() {
        assert_eq!(fired("(/ 1 3)", Dialect::CommonLisp), Vec::<&str>::new());
    }

    /// The three CLHS rules encode Common Lisp operator semantics and must not
    /// reach an Emacs Lisp file.
    #[test]
    fn the_common_lisp_rules_never_fire_on_emacs_lisp() {
        for source in [
            "(* 3.14 1.0d0)",
            "(do ((x 0.0 (+ x 0.1))) ((= x 1)))",
            "(truncate (coerce x 'double-float))",
        ] {
            assert_eq!(
                fired(source, Dialect::EmacsLisp),
                Vec::<&str>::new(),
                "{source}"
            );
        }
    }

    // -- the five quote shapes, through the real dispatch --------------------

    /// The dispatcher walks into quoted data like any other subtree. Each of
    /// these is a literal list, and every one of them fires without the
    /// `is_unevaluated_at` guard in each rule's `check`.
    #[test]
    fn no_rule_fires_on_a_hard_quoted_form() {
        for source in [
            "'(* 3.14 1.0d0)",
            "'(do ((x 0.0 (+ x 0.1))) ((= x 1)))",
            "'(truncate (coerce x 'double-float))",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source}"
            );
        }
        assert_eq!(fired("'(/ 1 3)", Dialect::EmacsLisp), Vec::<&str>::new());
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        assert_eq!(
            fired("(quote (* 3.14 1.0d0))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired("(quote (truncate (float x)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired("(quote (/ 1 3))", Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
    }

    /// A macro template: the form is built, not evaluated.
    #[test]
    fn no_rule_fires_inside_a_quasiquoted_macro_template() {
        assert_eq!(
            fired("`(* 3.14 1.0d0)", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired("`(truncate (float x))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    /// An unquote escapes back to code, so the form *is* evaluated and the rule
    /// must fire. This is the half of the quote model that a "skip anything
    /// under a quote" implementation gets wrong.
    #[test]
    fn a_rule_fires_again_under_an_unquote_inside_a_quasiquote() {
        assert_eq!(
            fired("`(a ,(* 3.14 1.0d0))", Dialect::CommonLisp),
            vec!["mixed-float-precision-arithmetic"]
        );
        assert_eq!(
            fired("`(a ,(truncate (float x)))", Dialect::CommonLisp),
            vec!["redundant-precision-coercion"]
        );
    }

    /// A comma inside a *hard* quote is a literal comma in a literal list, so
    /// everything under it stays data. This is the shape a single depth counter
    /// gets wrong.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        assert_eq!(
            fired("'(a ,(* 3.14 1.0d0))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired("'(a ,(truncate (float x)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    /// A node one level *inside* a quote is still data.
    #[test]
    fn no_rule_fires_deep_inside_a_quoted_list() {
        assert_eq!(
            fired("'(a (b (c (* 3.14 1.0d0))))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    // -- string literals -----------------------------------------------------

    /// A string is one atom, so nothing spelled inside one is ever a form.
    #[test]
    fn no_rule_fires_on_a_defect_spelled_inside_a_string_literal() {
        for source in [
            "(format nil \"(* 3.14 1.0d0)\")",
            "(format nil \"(truncate (coerce x 'double-float))\")",
            "(format nil \"(do ((x 0.0 (+ x 0.1))) ((= x 1)))\")",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source}"
            );
        }
        assert_eq!(
            fired("(message \"(/ 1 3)\")", Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
    }
}

/// The `eq`/`eql` family's quote model, driven through the *real* engine.
///
/// These three rules guard on a **hard** quote only, which is a weaker guard
/// than the float-precision rules' `is_unevaluated_at`, and the asymmetry is the
/// whole point: `'((eq function "f_eq") …)` is a data table and `` `(eq ,val 0)
/// `` is a macro template that becomes code. Both directions are pinned here,
/// because a fix for one is the standard way to break the other.
#[cfg(test)]
mod eq_family_quote_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 3] = [
        RuleEntry::new(
            &crate::eq_char_comparison::rule::META,
            &crate::eq_char_comparison::rule::RULE,
        ),
        RuleEntry::new(
            &crate::eq_number_comparison::rule::META,
            &crate::eq_number_comparison::rule::RULE,
        ),
        RuleEntry::new(
            &crate::eql_string_comparison::rule::META,
            &crate::eql_string_comparison::rule::RULE,
        ),
    ];

    fn fired(source: &str) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            Dialect::CommonLisp,
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

    // -- the rules still fire on real code (no over-suppression) -------------

    /// The control. Without these passing, every assertion below is vacuous.
    #[test]
    fn each_rule_fires_on_an_unquoted_call() {
        assert_eq!(
            fired("(defun f (x) (eq x \"done\"))"),
            vec!["eql-string-comparison"]
        );
        assert_eq!(
            fired("(defun f (x) (eql x \"done\"))"),
            vec!["eql-string-comparison"]
        );
        assert_eq!(
            fired("(defun f (x) (eq x 42))"),
            vec!["eq-number-comparison"]
        );
        assert_eq!(
            fired("(defun f (x) (eq x #\\a))"),
            vec!["eq-char-comparison"]
        );
    }

    /// The genuine float defect this rule exists for, as SBCL's own
    /// `cross-float.lisp:28` spells it. Dropping to zero findings on the corpus
    /// would mean this stopped working.
    #[test]
    fn a_negative_zero_float_comparison_is_still_reported() {
        assert_eq!(
            fired("(defun f (flonum) (eq flonum -0.0f0))"),
            vec!["eq-number-comparison"]
        );
        assert_eq!(
            fired("(defun f (flonum) (eq flonum -0.0d0))"),
            vec!["eq-number-comparison"]
        );
    }

    /// A quasiquoted macro template really does become code, so it must keep
    /// firing. This is the false *negative* that suppressing `quasi` would
    /// introduce — the shape of `hashset.lisp`'s `` `(eq ,val 0) ``.
    #[test]
    fn a_quasiquoted_macro_template_still_fires() {
        assert_eq!(fired("`(eq ,val 0)"), vec!["eq-number-comparison"]);
        assert_eq!(fired("`(eq ,name \"done\")"), vec!["eql-string-comparison"]);
        assert_eq!(fired("`(eq ,ch #\\a)"), vec!["eq-char-comparison"]);
    }

    /// A quasiquote with no unquote at all is still a template that becomes
    /// code, and is deliberately *not* suppressed.
    #[test]
    fn a_quasiquote_without_an_unquote_still_fires() {
        assert_eq!(fired("`(eq x \"done\")"), vec!["eql-string-comparison"]);
    }

    /// A `defmacro` body is ordinary code until something quotes it.
    #[test]
    fn a_call_in_an_unquoted_defmacro_body_still_fires() {
        assert_eq!(
            fired("(defmacro m (x) (when (eq x \"done\") (error \"no\")))"),
            vec!["eql-string-comparison"]
        );
        assert_eq!(
            fired("(defmacro m (x) `(if ,(eq x \"done\") 1 2))"),
            vec!["eql-string-comparison"]
        );
    }

    // -- hard-quoted data is not a call --------------------------------------

    /// The measured false positive: mgl-pax's HyperSpec index table, whose rows
    /// are `(symbol locative filename)` triples inside one `'(…)`.
    #[test]
    fn a_hard_quoted_data_table_yields_nothing() {
        assert_eq!(
            fired(
                "(defparameter *hyperspec-definitions*\n  '((eq function \"f_eq\")\n    (eql function \"f_eql\")\n    (eql type \"t_eql\")))"
            ),
            Vec::<&str>::new()
        );
    }

    /// The other measured false positive: an opcode-to-runtime alist, whose
    /// entries are dotted pairs inside one `'(…)`.
    #[test]
    fn a_hard_quoted_dotted_alist_entry_yields_nothing() {
        assert_eq!(
            fired("(dolist (entry '((eql . \"RT-EQL\") (equal . \"RT-EQUAL\")))\n  (use entry))"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn no_rule_fires_on_a_hard_quoted_form() {
        for source in ["'(eq x \"done\")", "'(eq x 42)", "'(eq x #\\a)"] {
            assert_eq!(fired(source), Vec::<&str>::new(), "{source}");
        }
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        assert_eq!(fired("(quote (eq x \"done\"))"), Vec::<&str>::new());
        assert_eq!(fired("(quote (eq x 42))"), Vec::<&str>::new());
    }

    /// A comma inside a *hard* quote is a literal comma in a literal list, so
    /// everything under it stays data. This is the shape a single `i32` depth
    /// counter gets wrong, and it is why `hard` is a `bool` that never clears.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        assert_eq!(fired("'(a ,(eq x \"done\"))"), Vec::<&str>::new());
        assert_eq!(fired("'(a ,(eq x 42))"), Vec::<&str>::new());
    }

    /// A node several levels inside a quote is still data.
    #[test]
    fn no_rule_fires_deep_inside_a_quoted_list() {
        assert_eq!(fired("'(a (b (c (eq x \"done\"))))"), Vec::<&str>::new());
    }

    // -- a one-argument form is not a comparison ------------------------------

    /// trivia's match patterns are one-argument by construction:
    /// `(is-match #\a (eq #\a))` reads `(eq #\a)` as a *pattern*, not a call.
    /// SBCL warns that a one-argument `eq` "wants exactly two", so whatever such
    /// a form is, it is not the bug these rules are named for.
    #[test]
    fn a_one_argument_form_is_not_reported() {
        for source in [
            "(is-match #\\a (eq #\\a))",
            "(is-match 1 (eq 1))",
            "(is-match \"s\" (eql \"s\"))",
        ] {
            assert_eq!(fired(source), Vec::<&str>::new(), "{source}");
        }
    }

    /// The arity guard must not swallow the two-argument call it sits next to.
    #[test]
    fn the_arity_guard_leaves_a_two_argument_call_alone() {
        assert_eq!(
            fired("(progn (eq #\\a) (eq ch #\\a))"),
            vec!["eq-char-comparison"]
        );
    }

    // -- string literals ------------------------------------------------------

    /// A string is one atom, so nothing spelled inside one is ever a form.
    #[test]
    fn no_rule_fires_on_a_defect_spelled_inside_a_string_literal() {
        assert_eq!(
            fired("(format nil \"(eq x \\\"done\\\")\")"),
            Vec::<&str>::new()
        );
    }
}

/// The false-positive sweep: realistic *correct* numeric code must produce no
/// findings at all, and the same code with the defects introduced must produce
/// exactly the expected ones.
///
/// A pair rather than a single corpus on purpose. A clean corpus alone proves
/// only that the rules are quiet, which a rule that never fires also achieves;
/// the dangerous twin is what makes the clean half discriminating. Each rule's
/// *denominator* is asserted non-zero too, so the corpus cannot drift into
/// exercising nothing while still passing.
#[cfg(test)]
mod corpus_sweep_tests {
    use std::path::Path;

    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use serde_json::json;

    use super::engine_pass_tests::fired;

    /// Realistic, correct Common Lisp numeric code. Every head all three CLHS
    /// rules anchor on appears here, used the way it should be.
    const REALISTIC_COMMON_LISP: &str = r#"
(defpackage :geometry (:use :cl))
(in-package :geometry)

(defconstant +tau+ 6.283185307179586d0)

(defun circle-area (radius)
  "Uniform double precision throughout, so nothing is capped."
  (* 3.141592653589793d0 radius radius))

(defun scale-by-half (x)
  ;; 0.5 is exactly representable, so mixing it costs nothing.
  (* x 0.5))

(defun blend (a b weight)
  (+ (* a (- 1.0 weight)) (* b weight)))

(defun midpoint (a b)
  ;; Exact rational division: (/ 5 2) is 5/2, not 2.
  (/ (+ a b) 2))

(defun whole-units (quantity)
  (truncate quantity))

(defun cents (amount)
  (round (* amount 100)))

(defun as-double (n)
  (coerce n 'double-float))

(defun remainder-of (a b)
  (floor a b))

(defun sweep (limit step)
  (do ((x 0.0 (+ x step))
       (acc nil))
      ((>= x limit) (nreverse acc))
    (push x acc)))

(defun count-down (n)
  (do ((i n (- i 1)))
      ((= i 0) :done)
    (report i)))

(defun exact-steps ()
  ;; 0.25 accumulates without drift, so the equality test is sound.
  (do ((x 0.0 (+ x 0.25)))
      ((= x 1.0) x)))

(defun grid (n)
  (dotimes (i n)
    (emit (* i 0.1))))
"#;

    /// The dangerous twin: the same shapes with each defect introduced exactly
    /// once.
    const DANGEROUS_TWIN_COMMON_LISP: &str = r#"
(defun circle-area (radius)
  (* 3.14 1.0d0 radius))

(defun whole-units (quantity)
  (truncate (coerce quantity 'double-float)))

(defun sweep (limit)
  (do ((x 0.0 (+ x 0.1)))
      ((= x limit))
    (emit x)))
"#;

    /// Realistic, correct Emacs Lisp. Every division here either keeps a
    /// non-zero quotient, is exact, or has an operand this layer cannot read.
    const REALISTIC_EMACS_LISP: &str = r#"
;;; geometry.el --- numeric helpers  -*- lexical-binding: t -*-

(defun geo-half (n)
  (/ n 2.0))

(defun geo-third (n)
  (/ n 3))

(defun geo-percent (part whole)
  (/ (* part 100) whole))

(defun geo-two-thirds ()
  (/ 200 3))

(defun geo-halve-exact ()
  (/ 6 3))

(defun geo-scale (n)
  (/ (float n) 4))
"#;

    /// The dangerous twin for Emacs Lisp.
    const DANGEROUS_TWIN_EMACS_LISP: &str = "(defun geo-third () (/ 1 3))\n";

    /// The four denominators for one Common Lisp source.
    fn common_lisp_denominators(source: &str) -> (u64, u64, u64) {
        let tree =
            SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse corpus");
        let path = Path::new("corpus.lisp");
        let arithmetic = crate::mixed_float_precision_arithmetic::domain::
            build_mixed_float_precision_arithmetic_report(path, Dialect::CommonLisp, &tree)
            .expect("report");
        let loops = crate::epsilon_less_float_loop_bound::domain::
            build_epsilon_less_float_loop_bound_report(path, Dialect::CommonLisp, &tree)
            .expect("report");
        let truncations =
            crate::redundant_precision_coercion::domain::build_redundant_precision_coercion_report(
                path,
                Dialect::CommonLisp,
                &tree,
            )
            .expect("report");
        (
            summary_of(&arithmetic.summary, "arithmetic_form_count"),
            summary_of(&loops.summary, "do_form_count"),
            summary_of(&truncations.summary, "truncation_form_count"),
        )
    }

    fn summary_of(summary: &[(&'static str, serde_json::Value)], key: &str) -> u64 {
        summary
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.as_u64())
            .unwrap_or_else(|| panic!("{key} in the summary"))
    }

    // -- the corpus actually exercises each rule -----------------------------

    /// Counting candidates, not findings. A corpus that produced no findings
    /// because it contained none of the relevant *heads* would prove nothing
    /// about false positives.
    #[test]
    fn the_realistic_corpus_exercises_every_rules_head() {
        let (arithmetic, loops, truncations) = common_lisp_denominators(REALISTIC_COMMON_LISP);
        assert!(arithmetic >= 10, "arithmetic candidates: {arithmetic}");
        assert!(loops >= 3, "do candidates: {loops}");
        assert!(truncations >= 3, "truncation candidates: {truncations}");

        let tree = SyntaxTree::parse_with_dialect(REALISTIC_EMACS_LISP, Dialect::EmacsLisp)
            .expect("parse corpus");
        let divisions = crate::division_result_precision_loss::domain::
            build_division_result_precision_loss_report(
                Path::new("corpus.el"),
                Dialect::EmacsLisp,
                &tree,
            )
            .expect("report");
        assert_eq!(
            summary_of(&divisions.summary, "division_form_count"),
            6,
            "division candidates"
        );
    }

    // -- no false positives on correct code ----------------------------------

    #[test]
    fn realistic_correct_common_lisp_produces_no_findings() {
        assert_eq!(
            fired(REALISTIC_COMMON_LISP, Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn realistic_correct_emacs_lisp_produces_no_findings() {
        assert_eq!(
            fired(REALISTIC_EMACS_LISP, Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
    }

    // -- the dangerous twin ---------------------------------------------------

    #[test]
    fn the_dangerous_twin_trips_every_common_lisp_rule_exactly_once() {
        assert_eq!(
            fired(DANGEROUS_TWIN_COMMON_LISP, Dialect::CommonLisp),
            vec![
                "epsilon-less-float-loop-bound",
                "mixed-float-precision-arithmetic",
                "redundant-precision-coercion",
            ]
        );
    }

    #[test]
    fn the_dangerous_twin_trips_the_emacs_lisp_rule() {
        assert_eq!(
            fired(DANGEROUS_TWIN_EMACS_LISP, Dialect::EmacsLisp),
            vec!["division-result-precision-loss"]
        );
    }

    // -- the repository's own fixtures ---------------------------------------

    /// Every `.lisp` and `.el` file the repository ships, through the real
    /// engine. None of them was written to contain any of these four defects,
    /// so any finding here is a false positive by construction.
    ///
    /// The file count is asserted so the sweep cannot pass by silently finding
    /// nothing to read — the failure mode that makes a corpus test worthless.
    #[test]
    fn the_repositorys_own_fixtures_produce_no_findings() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..");
        let mut scanned = 0;
        let mut findings: Vec<(String, Vec<&'static str>)> = Vec::new();

        for directory in ["tests/fixtures", "fuzz/corpus"] {
            let mut stack = vec![root.join(directory)];
            while let Some(current) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&current) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    let dialect = match path.extension().and_then(|e| e.to_str()) {
                        Some("lisp") => Dialect::CommonLisp,
                        Some("el") => Dialect::EmacsLisp,
                        _ => continue,
                    };
                    let Ok(source) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    // A fixture that deliberately does not parse is some other
                    // test's subject, not this one's.
                    if SyntaxTree::parse_with_dialect(&source, dialect).is_err() {
                        continue;
                    }
                    scanned += 1;
                    let names = fired(&source, dialect);
                    if !names.is_empty() {
                        findings.push((path.display().to_string(), names));
                    }
                }
            }
        }

        assert!(
            scanned >= 15,
            "the sweep must actually read the fixtures; scanned {scanned}"
        );
        assert!(
            findings.is_empty(),
            "false positives on the repository's own fixtures: {findings:?}"
        );
    }

    /// The summary shape the reports publish, pinned so a denominator rename is
    /// a test failure rather than a silently missing column.
    #[test]
    fn each_report_publishes_its_denominator_under_a_stable_name() {
        let tree = SyntaxTree::parse_with_dialect("(+ a b)", Dialect::CommonLisp).expect("parse");
        let path = Path::new("t.lisp");
        assert_eq!(
            crate::mixed_float_precision_arithmetic::domain::
                build_mixed_float_precision_arithmetic_report(path, Dialect::CommonLisp, &tree)
                .expect("report")
                .summary,
            vec![("arithmetic_form_count", json!(1))]
        );
    }
}

/// The quote model of the two rules whose findings a nested `'` makes
/// meaningless, driven through the *real* engine.
///
/// Both guard on a **hard** quote only, which is weaker than the float-precision
/// rules' `is_unevaluated_at`, and the asymmetry is the whole point. Measured
/// over 5,506 Common Lisp files: `equality-arity` reported 674 findings inside a
/// hard quote — a third of its output, nearly all of them `(eql x)` *type
/// specifiers* in `'(cons (eql function) null)` — and `one-step-arithmetic`
/// reported 163, several of them the expected value of a test asserting what
/// `1+` expands to, which its auto-fix would have rewritten into the assertion's
/// own negation.
///
/// Every suppression below is paired with a control proving the rule still fires
/// on the same shape as code, because over-suppression is the standard way to
/// "fix" a false positive and buy a false negative instead.
#[cfg(test)]
mod arity_and_shorthand_quote_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 2] = [
        RuleEntry::new(
            &crate::equality_arity::rule::META,
            &crate::equality_arity::rule::RULE,
        ),
        RuleEntry::new(
            &crate::one_step_arithmetic::rule::META,
            &crate::one_step_arithmetic::rule::RULE,
        ),
    ];

    fn fired(source: &str) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            Dialect::CommonLisp,
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

    // -- the controls: both rules still fire on code ------------------------

    #[test]
    fn a_misarity_equality_call_in_plain_code_is_still_reported() {
        assert_eq!(fired("(defun f (x) (eql x))"), vec!["equality-arity"]);
    }

    #[test]
    fn a_one_step_addition_in_plain_code_is_still_reported() {
        assert_eq!(fired("(defun f (x) (+ x 1))"), vec!["one-step-arithmetic"]);
    }

    /// The false negative a `quasi`-suppressing guard would buy, and the test
    /// that fails if this guard is ever "simplified" to `is_unevaluated_at`.
    ///
    /// SBCL spells this shape throughout — `` `(eq ,val 0) `` in `hashset.lisp`,
    /// and the same in `srctran.lisp` and `ir1tran-lambda.lisp` — and a template
    /// really does become code, so a quasiquoted ancestor must not suppress.
    #[test]
    fn a_quasiquoted_template_is_code_and_stays_reported() {
        assert_eq!(fired("`(a (eql x))"), vec!["equality-arity"]);
        assert_eq!(fired("`(a (+ x 1))"), vec!["one-step-arithmetic"]);
    }

    #[test]
    fn a_defmacro_body_building_a_form_stays_reported() {
        assert_eq!(
            fired("(defmacro m (v) `(a (eql v)))"),
            vec!["equality-arity"]
        );
        assert_eq!(
            fired("(defmacro m (v) `(a (+ v 1)))"),
            vec!["one-step-arithmetic"]
        );
    }

    /// An unquote escapes back to code several levels down, so the guard must
    /// not simply ask whether *any* ancestor was quoted.
    ///
    /// Only `one-step-arithmetic` can witness this: `equality-arity`'s own
    /// `examine_call` has long declined any node carrying a reader prefix of its
    /// own, so `,(eql x)` never reaches the guard added here.
    #[test]
    fn an_unquoted_form_inside_a_template_stays_reported() {
        assert_eq!(fired("`(+ ,x 1)"), vec!["one-step-arithmetic"]);
    }

    // -- the suppressions ---------------------------------------------------

    /// The shape that dominated the corpus: a quoted CLHS type specifier, in
    /// which a one-argument `eql` is correct Common Lisp.
    #[test]
    fn a_quoted_type_specifier_is_data_and_is_not_reported() {
        assert!(fired("(typep x '(cons (eql function) null))").is_empty());
    }

    #[test]
    fn a_quoted_arithmetic_form_is_data_and_is_not_reported() {
        assert!(fired("(assert-equal '(1+ n) (expand '(+ n 1)))").is_empty());
    }

    /// The long-hand `(quote …)` the reader also produces and macro output
    /// spells out.
    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert!(fired("(quote (a (eql x)))").is_empty());
        assert!(fired("(quote (a (+ x 1)))").is_empty());
    }

    /// The shape a single depth counter gets wrong: a comma inside a **hard**
    /// quote is a literal comma in a literal list, not an escape back to code.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(fired("'(a ,(eql x))").is_empty());
        assert!(fired("'(a ,(+ x 1))").is_empty());
    }

    /// Depth is not the discriminator: nesting does not restore evaluation.
    #[test]
    fn a_deeply_nested_quoted_form_is_still_data() {
        assert!(fired("'(a (b (c (eql x))))").is_empty());
        assert!(fired("'(a (b (c (+ x 1))))").is_empty());
    }

    /// A quoted sibling must not silence a real finding in the same file.
    #[test]
    fn quoting_one_form_does_not_suppress_its_unquoted_neighbour() {
        assert_eq!(fired("(list '(eql a) (eql b))"), vec!["equality-arity"]);
        assert_eq!(
            fired("(list '(+ a 1) (+ b 1))"),
            vec!["one-step-arithmetic"]
        );
    }
}

/// `equality-arity`'s type-specifier model, driven through the *real* engine.
///
/// CLHS 4.2.3 makes `(eql object)` a compound **type specifier**, so in a type
/// position one written argument is not a defect but the only legal spelling.
/// Measured over the same 5 506-file Common Lisp corpus the quote guard was
/// measured on, this rule reported 1 307 findings outside a hard quote, and
/// **679** of them were a one-argument `eql` sitting in an unevaluated type
/// position — a `defmethod` specializer, a `typecase` clause head, a
/// `declare`/`declaim` `type` specifier, a slot `:type`, a `the`, or a
/// `check-type`. Adjudicating 120 of the 1 307 individually found *no* genuine
/// arity error among them at all.
///
/// Every suppression below is paired with a control proving the rule still fires
/// on the same operator in a real call position, because buying a false negative
/// is the standard way to "fix" a false positive. The guard is deliberately
/// narrowed to `eql`-with-exactly-one-argument: `eq`, `equal` and `equalp` name
/// no type, and `(eql a b c)` names none either, so no other misarity shape can
/// reach it.
#[cfg(test)]
mod equality_arity_type_specifier_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
        &crate::equality_arity::rule::META,
        &crate::equality_arity::rule::RULE,
    )];

    fn fired(source: &str) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
    }

    fn reported(source: &str) -> bool {
        !fired(source).is_empty()
    }

    // -- controls: a genuine arity error in call position stays reported -----

    /// The shapes the rule exists for. Each is a real defect, and none of them
    /// is a type specifier, so the guard must not reach any of them.
    #[test]
    fn a_genuine_misarity_call_is_still_reported() {
        assert!(reported("(defun f (x) (eq x))"));
        assert!(reported("(defun f (x) (eql x))"));
        assert!(reported("(defun f () (eql))"));
        assert!(reported("(defun f (a b c) (eql a b c))"));
        assert!(reported("(defun f (a) (equal a))"));
        assert!(reported("(defun f (a b c d) (equalp a b c d))"));
    }

    /// A body form is a call, however deeply the body nests.
    #[test]
    fn a_misarity_call_in_a_progn_body_is_still_reported() {
        assert!(reported("(defun f (x) (progn (eql x)))"));
        assert!(reported("(progn (eq x))"));
    }

    /// A quasiquoted template really does become code, so it stays reported —
    /// the same asymmetry the hard-quote guard is built around.
    #[test]
    fn a_quasiquoted_template_stays_reported() {
        assert!(reported("(defmacro m (v) `(when ,v (eql ,v)))"));
    }

    /// `typep`, `subtypep` and `coerce` are *functions*: their type argument is
    /// an ordinary evaluated form, so an unquoted `(eql x)` there is a real
    /// one-argument call and anchoring on those heads would be a false negative.
    #[test]
    fn an_evaluated_type_argument_is_a_call_and_stays_reported() {
        assert!(reported("(defun f (x y) (typep x (eql y)))"));
        assert!(reported("(defun f (y) (subtypep (eql y) 'integer))"));
        assert!(reported("(defun f (x y) (coerce x (eql y)))"));
    }

    /// `and`/`or`/`not` are ordinary macros as well as type combinators, so
    /// climbing through one must not on its own suppress anything.
    #[test]
    fn a_misarity_call_under_a_bare_and_or_not_stays_reported() {
        assert!(reported("(defun f (x y) (when (or (eql x) y) 1))"));
        assert!(reported("(defun f (x) (and (not (eql x)) 1))"));
        assert!(reported("(defun f (x) (cons (eql x) nil))"));
    }

    /// A `defmethod` *body* is code, and so is an ordinary unspecialized
    /// parameter list; only the specializer is a type.
    #[test]
    fn a_misarity_call_in_a_defmethod_body_stays_reported() {
        assert!(reported("(defmethod g ((x integer)) (eql x))"));
    }

    /// The two-element shape alone is not enough: `(f (eql x))` is a call to `f`
    /// with a bad argument, not a specializer.
    #[test]
    fn a_two_element_list_outside_a_lambda_list_stays_reported() {
        assert!(reported("(defun f (x) (list (g (eql x))))"));
    }

    /// A specialized required parameter is exactly `(var specializer)`. Anything
    /// else is a malformed lambda list, and half-written code is precisely when
    /// a linter is read, so the malformed shape must not be granted the
    /// specializer's exemption.
    #[test]
    fn a_malformed_specialized_parameter_stays_reported() {
        assert!(reported("(defmethod g ((x (eql 1) extra)) x)"));
        assert!(reported("(defmethod g (((eql 1) x)) x)"));
    }

    /// Only a *required* parameter may be specialized. Past a lambda-list
    /// keyword the identical `(name form)` shape is an `&optional`/`&key`
    /// parameter whose second element is a **default value form** — live code,
    /// and a real one-argument call to `eql`.
    #[test]
    fn an_optional_or_key_parameter_default_is_code_and_stays_reported() {
        assert!(reported("(defmethod g (a &key (k (eql y))) k)"));
        assert!(reported("(defmethod g (a &optional (o (eql y))) o)"));
        assert!(reported("(defmethod g ((a integer) &key (k (eql y))) k)"));
    }

    /// The definer's head is load-bearing: an arbitrary macro whose arguments
    /// happen to nest a two-element list is not a generic function.
    #[test]
    fn the_specializer_shape_under_a_non_method_definer_stays_reported() {
        assert!(reported("(my-macro name (a (b (eql 1))))"));
    }

    /// The lambda list is the one the *method* dispatches on. The same shape in
    /// the body is an ordinary call.
    #[test]
    fn the_specializer_shape_in_a_method_body_stays_reported() {
        assert!(reported("(defmethod g ((x integer)) (foo (a (eql 1))))"));
    }

    /// A `typecase` keyform is code even when it is itself a list, so the clause
    /// anchor must start at the first clause and not at the keyform.
    #[test]
    fn a_list_shaped_typecase_keyform_stays_reported() {
        assert!(reported("(typecase ((eql x) 1) (integer 2))"));
    }

    /// Only `eql` names a type. The guard must be unreachable for the other
    /// three predicates even in a genuine type position.
    #[test]
    fn a_non_eql_predicate_in_a_specializer_shape_stays_reported() {
        assert!(reported("(defmethod g ((x (eq 7))) x)"));
        assert!(reported("(defmethod g ((x (equal 7))) x)"));
    }

    /// `(eql a b)` is a *valid* two-argument call and `(eql a b c)` is a defect;
    /// neither is a type specifier, so the specializer shape must not hide the
    /// three-argument one.
    #[test]
    fn a_multi_argument_eql_in_a_specializer_shape_stays_reported() {
        assert!(reported("(defmethod g ((x (eql 1 2 3))) x)"));
    }

    // -- the suppressions ----------------------------------------------------

    /// `(defmethod g ((x (eql 7))) …)`: CLHS's `eql` specializer, and the single
    /// largest false-positive shape in the corpus.
    #[test]
    fn a_defmethod_eql_specializer_is_a_type_and_is_not_reported() {
        assert!(!reported("(defmethod g ((x (eql 7))) x)"));
        assert!(!reported("(defmethod g ((x (eql :key)) y) (list x y))"));
    }

    /// The qualifier shifts the lambda list one child right, and a `(setf …)`
    /// method name is itself a list — the two shapes a fixed index gets wrong.
    #[test]
    fn a_qualified_or_setf_named_method_specializer_is_still_found() {
        assert!(!reported("(defmethod g :around ((x (eql 7))) x)"));
        assert!(!reported("(defmethod (setf g) (v (x (eql 7))) v)"));
        assert!(!reported("(defmethod g :before ((a t) (x (eql 'k))) x)"));
    }

    /// `(:method …)` inside `defgeneric` is a `defmethod` with the head
    /// replaced, and its lambda list starts one child earlier.
    #[test]
    fn a_defgeneric_method_clause_specializer_is_not_reported() {
        assert!(!reported("(defgeneric g (x) (:method ((x (eql :a))) x))"));
    }

    /// The reader upcases and a package prefix does not change which operator is
    /// named, so both spellings have to be recognized.
    #[test]
    fn a_shouted_or_package_qualified_definer_is_still_recognized() {
        assert!(!reported("(DEFMETHOD G ((X (EQL 7))) X)"));
        assert!(!reported("(cl:defmethod g ((x (eql 7))) x)"));
    }

    /// A `typecase` clause is headed by a type specifier, not a call.
    #[test]
    fn a_typecase_clause_head_is_a_type_and_is_not_reported() {
        assert!(!reported("(typecase x ((eql 5) 1) (t 2))"));
        assert!(!reported("(etypecase x ((eql 5) 1))"));
        assert!(!reported("(ctypecase x ((eql 5) 1))"));
    }

    /// The `typecase` *keyform* is an ordinary evaluated form, so a bad call
    /// there is still a defect — the clause heads are the only type positions.
    #[test]
    fn the_typecase_keyform_is_code_and_stays_reported() {
        assert!(reported("(typecase (eql x) (integer 1))"));
    }

    /// A `case` clause head is a set of *object* keys, not a type, so the
    /// `typecase` anchor must not extend to it.
    ///
    /// The rule itself no longer reports this shape, but for an unrelated
    /// reason: a clause key is not a *call*, which the key-and-binding guard
    /// settles independently. The invariant this module is about — that the
    /// *type* anchor stops at `typecase` — is therefore asserted directly
    /// against the type predicate, in
    /// `support::tests::a_case_clause_key_is_not_a_type_specifier_position`.
    #[test]
    fn a_case_clause_head_is_not_reported_as_a_call() {
        assert!(!reported("(case x ((eql 5) 1))"));
    }

    /// The compound specifiers nest, and the corpus writes the nesting far more
    /// often than the bare form: `((cons (eql :begin-file)) …)` is SBCL's own.
    #[test]
    fn a_nested_compound_type_specifier_is_reached_through_its_combinators() {
        assert!(!reported("(typecase x ((cons (eql :begin-file)) 1))"));
        assert!(!reported("(etypecase x ((or null (eql t)) 1))"));
        assert!(!reported(
            "(typecase x ((or function (cons (eql function))) 1))"
        ));
        assert!(!reported("(typecase x ((and integer (not (eql 0))) 1))"));
    }

    /// `(declare (type SPEC var))` and its `declaim`/`proclaim` siblings.
    #[test]
    fn a_type_declaration_specifier_is_not_reported() {
        assert!(!reported("(defun f (x) (declare (type (eql 0) x)) x)"));
        assert!(!reported(
            "(defun f (x) (declare (type (or function (eql 0)) x)) x)"
        ));
        assert!(!reported("(declaim (type (or list (eql t)) *v*))"));
    }

    /// A function may legitimately be named `type`, so the declaration head has
    /// to agree before the `(type …)` shape means anything.
    #[test]
    fn a_bare_type_call_outside_a_declaration_stays_reported() {
        assert!(reported("(defun f (x) (type (eql x)))"));
    }

    /// `the` and `check-type` are the two standard forms that take a specifier
    /// without evaluating it.
    #[test]
    fn an_unevaluated_specifier_argument_is_not_reported() {
        assert!(!reported("(defun f (x) (the (eql 1) x))"));
        assert!(!reported(
            "(defun f (x) (check-type x (or (eql 2) (eql 4))))"
        ));
    }

    /// `the`'s *second* argument is the value form, which is code.
    #[test]
    fn the_value_form_of_a_the_is_code_and_stays_reported() {
        assert!(reported("(defun f (x) (the integer (eql x)))"));
    }

    /// A slot `:type`, as `defclass`, `defstruct` and `define-primitive-object`
    /// all spell it.
    #[test]
    fn a_slot_type_option_is_not_reported() {
        assert!(!reported(
            "(defclass c () ((s :type (or (eql :ok) (eql :fail)))))"
        ));
        assert!(!reported("(defstruct s (a nil :type (or list (eql 0))))"));
    }

    /// A specializer written inside a macro template is still a specializer, and
    /// the corpus writes exactly this to generate methods.
    #[test]
    fn a_specializer_inside_a_quasiquoted_template_is_not_reported() {
        assert!(!reported(
            "(defmacro m (name) `(defmethod g ((k (eql ',name))) k))"
        ));
    }

    /// Suppressing one form must not silence a real defect beside it.
    #[test]
    fn a_type_position_does_not_suppress_its_neighbour() {
        assert_eq!(
            fired("(defmethod g ((x (eql 7))) (eql x))"),
            vec!["equality-arity"]
        );
        assert_eq!(
            fired("(progn (typecase y ((eql 5) 1)) (eq z))"),
            vec!["equality-arity"]
        );
    }
}

/// `equality-arity`'s key- and binding-position model, driven through the
/// *real* engine.
///
/// A `case`-family clause **key** and a variable-binding list are positions
/// that name something rather than call it, so the written shape `(eql x)`
/// there has no arity to be wrong about. CLHS 5.3 gives `case` the syntax
/// `(case keyform {(keys form*)}*)`, so in `(case kind (eql x) …)` the `eql` is
/// a symbol being compared against and the `x` is a *body form*;
/// `(multiple-value-bind (equal certain) …)` binds two variables.
///
/// Measured over the same 5 556-file corpus (5 506 of them parsing as Common
/// Lisp) the earlier guards were measured on, this accounts for **224** of the
/// 631 findings that survive the hard-quote and type-specifier guards: 195
/// `case`-family clause keys, 25 `multiple-value-bind` variable lists and 4
/// `let` bindings. A 14-finding random sample of the 224 was adjudicated
/// against its real source line by line and contained **no** genuine arity
/// error.
///
/// Every suppression below is paired with a control proving the rule still
/// fires on the same operator in a real call position, because buying a false
/// negative is the standard way to "fix" a false positive. The sharpest of
/// those controls is [`a_key_named_eql_still_reports_a_call_in_its_own_body`]:
/// the clause *key* is silenced and the clause *body* is not, in one form.
#[cfg(test)]
mod equality_arity_key_and_binding_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
        &crate::equality_arity::rule::META,
        &crate::equality_arity::rule::RULE,
    )];

    /// How many `equality-arity` findings the real dispatch reports.
    fn count(source: &str) -> usize {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .len()
    }

    fn reported(source: &str) -> bool {
        count(source) > 0
    }

    // -- the controls: genuine arity errors are still reported ---------------

    /// The defect the rule is named for, in every shape the corpus work
    /// touched. If any of these stops firing, the guard has bought a false
    /// negative.
    #[test]
    fn a_genuine_arity_error_is_still_reported() {
        for source in [
            "(eq x)",
            "(eql)",
            "(equal a)",
            "(equalp a b c)",
            "(defun f (x) (eq x))",
            "(defun f (x) (eql a b c))",
            "(when (equal a) t)",
            // A quasiquoted template becomes a real call, and stays reported.
            // The candidate must not carry the quasiquote itself: `examine_call`
            // declines a node with its *own* reader prefix, here and on `main`
            // alike, so the template's inner form is the one under test.
            "(defmacro m (v) `(list (eq ,v)))",
        ] {
            assert!(reported(source), "{source}");
        }
    }

    /// `=` and `<` are variadic, so they are not this rule's business at all —
    /// the control that the guard did not widen the rule's reach.
    #[test]
    fn a_variadic_numeric_comparison_is_still_not_reported() {
        assert!(!reported("(= 1)"));
        assert!(!reported("(< a b c)"));
        assert!(!reported("(case k (= 1))"));
    }

    // -- case-family clause keys ---------------------------------------------

    #[test]
    fn a_case_family_clause_key_is_not_reported() {
        for source in [
            "(case kind (eql x) (equal y))",
            "(ecase test (eq (f)) (equalp (g)))",
            "(ccase test (eql (f)))",
            // The key-list spelling, including a singleton list.
            "(case std-fn ((eql char=) (f)) ((equal) (g)))",
            // Zero-length and long bodies are still bodies, not arguments.
            "(case k (eq))",
            "(case k (eql a b c))",
        ] {
            assert!(!reported(source), "{source}");
        }
    }

    /// **The boundary this change creates.** A clause *body* is ordinary code,
    /// and a real misarity call there must still be reported.
    #[test]
    fn a_call_in_a_case_clause_body_is_still_reported() {
        assert!(reported("(case kind (some-key (eq x)))"));
        assert!(reported("(case kind ((a b) (eql x)))"));
        assert!(reported("(case kind (otherwise (equal x)))"));
        assert!(reported("(case kind (t (eq x)))"));
        assert!(reported("(ecase kind (some-key (eq x)))"));
    }

    /// The sharpest form of that boundary: the key and the body name the same
    /// operator, so exactly one of the two nodes may be reported.
    #[test]
    fn a_key_named_eql_still_reports_a_call_in_its_own_body() {
        assert_eq!(count("(case kind (eql (eql x)))"), 1);
        assert_eq!(count("(case kind ((eql equal) (eq x)))"), 1);
    }

    /// The `keyform` is child 1 of the `case` and *is* evaluated.
    #[test]
    fn a_misarity_case_keyform_is_still_reported() {
        assert!(reported("(case (eql x) (a 1))"));
    }

    // -- binding positions ----------------------------------------------------

    #[test]
    fn a_variable_binding_list_is_not_reported() {
        for source in [
            "(multiple-value-bind (equal certain) (type= a b) certain)",
            "(multiple-value-bind (equal less greater when-true when-false) (f) equal)",
            "(destructuring-bind (eq a) form eq)",
            "(dolist (equal items) equal)",
            "(let (mark (eq (lambda-var-eq-constraints leaf))) eq)",
            "(let* ((equal (f))) equal)",
            "(defun f (equal x) x)",
        ] {
            assert!(!reported(source), "{source}");
        }
    }

    /// A binding's initial value form, a `do` step form and a `do` end test are
    /// all live code one level below the binding list.
    #[test]
    fn a_call_around_a_binding_list_is_still_reported() {
        assert!(reported("(let ((a (eql x))) a)"));
        assert!(reported("(do ((i 0 (eql i))) (done) x)"));
        assert!(reported("(do ((i 0)) ((eql i)) x)"));
        assert!(reported("(let ((a 1)) (eq a))"));
        assert!(reported("(multiple-value-bind (a b) (f) (eq a))"));
        assert!(reported("(flet ((g (x) (eql x))) (g 1))"));
    }

    /// A suppressed sibling must not silence a real finding in the same file.
    #[test]
    fn a_binding_list_does_not_suppress_its_neighbour() {
        assert_eq!(
            count("(multiple-value-bind (equal certain) (f) (eq certain))"),
            1
        );
        assert_eq!(count("(case k (eql 1))\n(eq x)\n"), 1);
    }
}
