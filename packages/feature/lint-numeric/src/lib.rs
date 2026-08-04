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
