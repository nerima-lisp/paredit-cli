//! Each rule against the code it must flag and the code it must not.
//!
//! Every case is paired. A rule that reported unconditionally would satisfy any
//! single positive test and be worthless, so each positive has a negative that
//! differs by exactly the thing the rule is about.
//!
//! Two suite-level tests carry the weight the per-rule pairs cannot:
//! [`a_realistic_correct_file_produces_no_findings`] sweeps a file written the
//! way the manual says and asserts *zero*, while
//! [`the_correct_file_contains_every_shape_the_rules_key_on`] pins a non-zero
//! candidate count so that zero cannot be a zero over nothing, and
//! [`the_dangerous_twin_fires_every_rule_exactly_once`] is the same file with
//! each idiom broken, asserting one finding per rule.

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path;

/// The rules of this crate, in one catalogue, so a test run dispatches through
/// the real engine rather than calling `check` directly. Calling `check`
/// directly would bypass the head index, which is where a wrong `HeadFilter`
/// or a forgotten `dialect_scope` shows up.
const CATALOG: [RuleEntry; 2] = [
    RuleEntry::new(
        &crate::process_filter_assumes_whole_output::rule::META,
        &crate::process_filter_assumes_whole_output::rule::RULE,
    ),
    RuleEntry::new(
        &crate::repeating_timer_handle_discarded::rule::META,
        &crate::repeating_timer_handle_discarded::rule::RULE,
    ),
];

const LEXICAL: &str = ";;; f.el --- x -*- lexical-binding: t -*-\n";

fn outcomes_in(dialect: Dialect, source: &str) -> Vec<LintOutcome> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("fixture parses");
    let catalog = RuleCatalog::new(&CATALOG);
    let index = build_head_index(catalog);
    collect_lint_outcomes(
        catalog,
        &index,
        Path::new("f.el"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("the engine runs")
}

/// The rule names that fire on `source` exactly as written.
fn rules_for_file(source: &str) -> Vec<&'static str> {
    outcomes_in(Dialect::EmacsLisp, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

/// The rule names that fire on `body`, which carries a lexical header.
fn rules_for(body: &str) -> Vec<&'static str> {
    rules_for_file(&format!("{LEXICAL}{body}"))
}

const NONE: [&str; 0] = [];

// ---------------------------------------------------------------------------
// Dialect scope
// ---------------------------------------------------------------------------

#[test]
fn every_rule_is_scoped_to_emacs_lisp() {
    use paredit_core_lint_engine::policy::RuleDialectScope;
    // The default scope is COMMON_LISP_ONLY, so a rule that forgot to override
    // it would silently never run on a `.el` file and no other test would say
    // so.
    for entry in &CATALOG {
        assert_eq!(
            entry.rule().dialect_scope(),
            RuleDialectScope::EMACS_LISP_ONLY,
            "{} is not scoped to Emacs Lisp",
            entry.meta().name()
        );
    }
}

#[test]
fn the_same_source_read_as_common_lisp_fires_nothing() {
    // Proves the scope is enforced by the engine, not merely declared.
    assert_eq!(
        outcomes_in(Dialect::CommonLisp, "(run-with-timer 0 60 #'my-refresh)\n").len(),
        0
    );
}

// ---------------------------------------------------------------------------
// Shared: the two-counter quote model
// ---------------------------------------------------------------------------

#[test]
fn a_quoted_call_is_data_and_is_left_alone() {
    assert_eq!(rules_for("'(run-with-timer 0 60 #'my-refresh)\n"), NONE);
}

#[test]
fn a_call_unquoted_back_into_a_backquote_is_still_code() {
    // The two-counter quote model earns itself here: a single depth counter
    // would call this data and miss the finding.
    assert_eq!(
        rules_for("`(a ,(run-with-timer 0 60 #'my-refresh) b)\n"),
        ["elisp-repeating-timer-handle-discarded"]
    );
}

#[test]
fn a_comma_inside_a_hard_quote_does_not_escape_back_to_code() {
    // Inside `'(…)` a comma is a comma character in a literal list. `hard`
    // never clearing is what models that.
    assert_eq!(
        rules_for("'(a ,(run-with-timer 0 60 #'my-refresh) b)\n"),
        NONE
    );
}

// ---------------------------------------------------------------------------
// elisp-process-filter-assumes-whole-output
// ---------------------------------------------------------------------------

#[test]
fn a_filter_parsing_its_chunk_as_json_is_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc string) (my-handle (json-parse-string string))))\n"
        ),
        ["elisp-process-filter-assumes-whole-output"]
    );
}

#[test]
fn a_filter_splitting_its_chunk_into_records_is_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc string) (dolist (l (split-string string \"\\n\")) (my-handle l))))\n"
        ),
        ["elisp-process-filter-assumes-whole-output"]
    );
}

#[test]
fn a_filter_that_inserts_its_chunk_into_a_buffer_is_not_reported() {
    // The buffer is the accumulator the manual prescribes.
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc string) (with-current-buffer (process-buffer proc) (insert string))))\n"
        ),
        NONE
    );
}

#[test]
fn a_filter_that_accumulates_before_parsing_is_not_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc string) (process-put proc :buf (concat (process-get proc :buf) string)) (my-drain proc)))\n"
        ),
        NONE
    );
}

/// The shape `affe.el:77` has: correct chunk stitching where what is
/// accumulated is *derived* from the chunk, never the chunk itself.
///
/// An earlier version of the rule required the chunk symbol to be a direct
/// argument of an accumulator and reported this, which is textbook-correct
/// code.
#[test]
fn a_filter_accumulating_a_value_derived_from_the_chunk_is_not_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (_ out) (let ((lines (split-string out \"\\n\"))) (if (not (cdr lines)) (setq rest (concat rest (car lines))) (setcar lines (concat rest (car lines))) (funcall cb lines)))))\n"
        ),
        NONE
    );
}

#[test]
fn a_chunk_reaching_the_parser_through_an_accumulator_is_not_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc string) (setq my-tail (concat my-tail string)) (json-parse-string my-tail)))\n"
        ),
        NONE
    );
}

#[test]
fn a_make_process_filter_keyword_is_read_too() {
    assert_eq!(
        rules_for(
            "(make-process :name \"x\" :command cmd :filter (lambda (proc string) (my-handle (json-parse-string string))))\n"
        ),
        ["elisp-process-filter-assumes-whole-output"]
    );
}

#[test]
fn a_make_network_process_filter_keyword_is_read_too() {
    assert_eq!(
        rules_for(
            "(make-network-process :name \"x\" :filter (lambda (proc string) (my-handle (json-read-from-string string))))\n"
        ),
        ["elisp-process-filter-assumes-whole-output"]
    );
}

#[test]
fn a_filter_that_ignores_its_chunk_entirely_is_not_reported() {
    assert_eq!(
        rules_for("(set-process-filter p (lambda (proc _string) (my-poll)))\n"),
        NONE
    );
}

/// An `_`-prefixed name is not a waiver when the body goes on to parse it.
///
/// This pins the removal of a guard that excluded such names: it killed no
/// test, and chasing why showed it suppressed a genuine instance of the defect.
#[test]
fn an_underscore_prefixed_chunk_that_is_parsed_anyway_is_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc _string) (my-handle (json-parse-string _string))))\n"
        ),
        ["elisp-process-filter-assumes-whole-output"]
    );
}

#[test]
fn a_filter_named_by_a_symbol_is_not_reported() {
    // This crate cannot follow the name to a definition, and guessing would
    // report code it has not read.
    assert_eq!(rules_for("(set-process-filter p #'my-filter)\n"), NONE);
}

/// A `'(lambda …)` is a list, not a function, so it is not a filter this rule
/// reads. `elisp-quoted-lambda` makes the stronger complaint about it.
#[test]
fn a_quoted_lambda_filter_is_not_read_as_a_filter() {
    assert_eq!(
        rules_for("(set-process-filter p '(lambda (proc string) (json-parse-string string)))\n"),
        NONE
    );
}

#[test]
fn a_quoted_filter_installation_is_data_and_is_left_alone() {
    assert_eq!(
        rules_for("'(set-process-filter p (lambda (proc string) (json-parse-string string)))\n"),
        NONE
    );
}

/// The consumer must take the chunk *itself*, not merely appear in the body.
#[test]
fn a_consumer_called_on_something_other_than_the_chunk_is_not_reported() {
    assert_eq!(
        rules_for(
            "(set-process-filter p (lambda (proc string) (my-log string) (json-parse-string (process-get proc :buf))))\n"
        ),
        NONE
    );
}

// ---------------------------------------------------------------------------
// elisp-repeating-timer-handle-discarded
// ---------------------------------------------------------------------------

#[test]
fn a_repeating_timer_whose_handle_is_dropped_is_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 0 60 #'my-refresh) (message \"started\"))\n"),
        ["elisp-repeating-timer-handle-discarded"]
    );
}

#[test]
fn run_at_time_and_run_with_idle_timer_are_read_too() {
    assert_eq!(
        rules_for(
            "(defun my-start () (run-at-time 0.5 0.5 #'my-a) (run-with-idle-timer 1 1 #'my-b) (message \"x\"))\n"
        ),
        [
            "elisp-repeating-timer-handle-discarded",
            "elisp-repeating-timer-handle-discarded"
        ]
    );
}

#[test]
fn a_top_level_repeating_timer_is_reported() {
    // Nothing reads the value of a form at file scope.
    assert_eq!(
        rules_for("(run-with-timer 0 60 #'my-refresh)\n"),
        ["elisp-repeating-timer-handle-discarded"]
    );
}

#[test]
fn a_one_shot_timer_is_not_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 5 nil #'my-once) (message \"x\"))\n"),
        NONE
    );
}

/// Measured in GNU Emacs 31.0.91: `REPEAT` of `0` yields
/// `timer--repeat-delay` nil, so it is a one-shot. The corpus caught this —
/// an earlier version reported `pulse.el:260`, which passes `0` deliberately.
#[test]
fn a_zero_repeat_is_a_one_shot_and_is_not_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 1 0 #'my-once) (message \"x\"))\n"),
        NONE
    );
}

/// Same measurement: `REPEAT` of `t` also yields `timer--repeat-delay` nil.
#[test]
fn a_t_repeat_is_a_one_shot_and_is_not_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 1 t #'my-once) (message \"x\"))\n"),
        NONE
    );
}

#[test]
fn a_negative_repeat_is_a_one_shot_and_is_not_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 1 -1 #'my-once) (message \"x\"))\n"),
        NONE
    );
}

#[test]
fn a_fractional_repeat_still_repeats_and_is_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 1 0.5 #'my-poll) (message \"x\"))\n"),
        ["elisp-repeating-timer-handle-discarded"]
    );
}

#[test]
fn a_repeating_timer_bound_by_let_is_not_reported() {
    assert_eq!(
        rules_for(
            "(defun my-start () (let ((tm (run-with-timer 0 60 #'my-refresh))) (my-remember tm)))\n"
        ),
        NONE
    );
}

#[test]
fn a_repeating_timer_stored_by_setq_is_not_reported() {
    assert_eq!(
        rules_for(
            "(defun my-start () (setq my-timer (run-with-timer 0 60 #'my-refresh)) (message \"x\"))\n"
        ),
        NONE
    );
}

#[test]
fn a_repeating_timer_pushed_onto_a_list_is_not_reported() {
    assert_eq!(
        rules_for(
            "(defun my-start () (push (run-with-timer 0 60 #'my-refresh) my-timers) (message \"x\"))\n"
        ),
        NONE
    );
}

#[test]
fn a_repeating_timer_returned_from_its_function_is_not_reported() {
    assert_eq!(
        rules_for("(defun my-start () (run-with-timer 0 60 #'my-refresh))\n"),
        NONE
    );
}

#[test]
fn a_repeat_argument_the_rule_cannot_read_is_not_guessed_at() {
    assert_eq!(
        rules_for(
            "(defun my-start () (run-with-timer 0 my-interval #'my-refresh) (message \"x\"))\n"
        ),
        NONE
    );
}

/// A computed REPEAT is a list, not an atom, and is likewise not guessed at.
#[test]
fn a_computed_repeat_argument_is_not_guessed_at() {
    assert_eq!(
        rules_for(
            "(defun my-start () (run-with-timer 0 (if x 60 nil) #'my-refresh) (message \"x\"))\n"
        ),
        NONE
    );
}

// ---------------------------------------------------------------------------
// The corpus pair
// ---------------------------------------------------------------------------

/// Realistic Emacs Lisp written the way the manual says.
///
/// Every shape each rule keys on appears here at least once, correctly used.
const CORRECT: &str = r#";;; good.el --- correct elisp -*- lexical-binding: t -*-

(defvar my-refresh-timer nil)
(defvar my-timers nil)

(defun my-start-refresh ()
  "Start the periodic refresh, keeping the handle so it can be stopped."
  (interactive)
  (setq my-refresh-timer (run-with-timer 0 60 #'my-refresh))
  my-refresh-timer)

(defun my-stop-refresh ()
  (interactive)
  (when (timerp my-refresh-timer)
    (cancel-timer my-refresh-timer)
    (setq my-refresh-timer nil)))

(defun my-schedule-many ()
  (push (run-at-time 1 30 #'my-a) my-timers)
  (push (run-with-idle-timer 2 15 #'my-b) my-timers))

(defun my-once ()
  "A one-shot needs no handle, and a zero repeat is a one-shot."
  (run-with-timer 5 nil #'my-deferred)
  (run-at-time 0.5 0 #'my-flush)
  (message "scheduled"))

(defun my-start-server ()
  "Accumulate on the process, then drain complete records."
  (make-process
   :name "my-server"
   :command '("my-server")
   :filter (lambda (proc chunk)
             (process-put proc :tail (concat (process-get proc :tail) chunk))
             (my-drain-complete-records proc))))

(defun my-connect (name callback)
  "Stitch partial lines across chunk boundaries."
  (let ((rest ""))
    (make-network-process
     :name name
     :filter (lambda (_ out)
               (let ((lines (split-string out "\n")))
                 (if (not (cdr lines))
                     (setq rest (concat rest (car lines)))
                   (setcar lines (concat rest (car lines)))
                   (funcall callback lines)))))))

(defun my-log-filter (proc chunk)
  "The buffer is the accumulator."
  (with-current-buffer (process-buffer proc)
    (goto-char (point-max))
    (insert chunk)))

(defun my-attach (proc)
  (set-process-filter proc #'my-log-filter))
"#;

/// The same file with each idiom broken, one per rule.
const DANGEROUS: &str = r#";;; bad.el --- the dangerous twin -*- lexical-binding: t -*-

(defun my-start-refresh ()
  "The handle goes nowhere, so nothing can cancel this."
  (run-with-timer 0 60 #'my-refresh)
  (message "started"))

(defun my-start-server ()
  "Parses a chunk as if it were a whole message, and keeps no state."
  (make-process
   :name "my-server"
   :command '("my-server")
   :filter (lambda (proc chunk)
             (my-handle (json-parse-string chunk)))))
"#;

#[test]
fn a_realistic_correct_file_produces_no_findings() {
    assert_eq!(rules_for_file(CORRECT), NONE);
}

/// The zero above must not be a zero over nothing.
///
/// A sweep that finds nothing because it *looked* at nothing is a false clean,
/// so this pins the candidate count each rule's head filter actually sees.
#[test]
fn the_correct_file_contains_every_shape_the_rules_key_on() {
    let occurrences = |needle: &str| CORRECT.matches(needle).count();
    let timer_candidates = occurrences("(run-with-timer")
        + occurrences("(run-at-time")
        + occurrences("(run-with-idle-timer");
    assert_eq!(timer_candidates, 5, "timer candidates");
    let filter_candidates = occurrences("(set-process-filter") + occurrences(":filter");
    assert_eq!(filter_candidates, 3, "process-filter candidates");
}

#[test]
fn the_dangerous_twin_fires_every_rule_exactly_once() {
    let mut fired = rules_for_file(DANGEROUS);
    fired.sort_unstable();
    assert_eq!(
        fired,
        [
            "elisp-process-filter-assumes-whole-output",
            "elisp-repeating-timer-handle-discarded",
        ]
    );
}
