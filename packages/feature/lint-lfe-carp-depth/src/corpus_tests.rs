//! Realistic LFE that must stay silent, and a dangerous twin of it that must
//! fire each rule exactly once.
//!
//! Self-contained on purpose: the third-party audit these were distilled from
//! ran against ~140 cloned repositories, which a test in this repository
//! cannot depend on. What travels with the package is the *shape* of that
//! corpus.
//!
//! # Why both halves, and why the candidate counts
//!
//! A clean sweep over code that contains none of the constructs a rule
//! adjudicates is a false clean, not a pass. So the correct half asserts a
//! **non-zero candidate count** alongside its zero findings: the rules were
//! asked a real question about this code and answered "no".
//!
//! The dangerous twin is the same code with one defect introduced per rule,
//! which is what proves the correct half is silent because the code is correct
//! rather than because the rules stopped working.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::engine_pass_tests::fired;
use crate::{dead_clause, illegal_guard_call};

/// Ordinary LFE: a gen_server-ish module using every construct both rules
/// adjudicate, and doing all of it correctly.
///
/// Contains, deliberately:
/// - matching `defun`s whose catch-all clause is last,
/// - traditional `defun`s whose argument lists must not be read as clauses,
/// - guards using unqualified BIFs, `erlang:`-qualified guard BIFs, and the
///   `(call 'erlang …)` and `(: erlang …)` spellings,
/// - a `receive` with an `after` timeout,
/// - a `defsyntax` whose template contains a `case` that would otherwise read
///   as having a dead clause,
/// - a `` ` `` macro template, and hard-quoted data holding both a `when` and a
///   `case`.
const CORRECT: &str = r#"
(defmodule store
  (behaviour gen_server)
  (export (start-link 0) (classify 1) (lookup 2) (loop 1) (handle-call 3)))

(defun start-link ()
  (gen_server:start_link (tuple 'local 'store) 'store '() '()))

;; Traditional defun: the argument list is not a clause list.
(defun lookup (key table)
  (case (dict:find key table)
    ((tuple 'ok value) value)
    ('error 'not-found)))

;; Matching defun with the catch-all last, and guards in three legal spellings.
(defun classify
  ((x) (when (is_integer x)) 'integer)
  ((x) (when (erlang:is_atom x)) 'atom)
  ((x) (when (call 'erlang 'is_list x)) 'list)
  ((x) (when (: erlang is_tuple x)) 'tuple)
  ((x) (when (andalso (is_binary x) (> (byte_size x) 0))) 'binary)
  ((_) 'unknown))

(defun handle-call
  ((`#(get ,key) _from state) (tuple 'reply (lookup key state) state))
  ((`#(put ,key ,value) _from state)
   (tuple 'reply 'ok (dict:store key value state)))
  ((_ _from state) (tuple 'reply 'unknown state)))

(defun loop (state)
  (receive
    ((tuple 'get key) (when (is_atom key)) (loop state))
    ('stop 'stopped)
    (other (when (is_tuple other)) (loop state))
    (after 5000 (loop state))))

;; A syntax-rules template. `p` here is a pattern variable, not a variable,
;; so the `_` after it is not a dead clause.
(defsyntax if-ok
  ([('ok p) . body] (case p (v . body) (_ 'error)))
  ([] 'error))

;; A macro template: real code, but its clause list is not all here.
(defmacro with-default (expr default)
  `(case ,expr
     ('undefined ,default)
     (v v)))

;; Hard-quoted data holding shapes that would otherwise be reported.
(defun forms ()
  (list '(when (lists:member x '(1 2)))
        '(case y (_ 'any) ('two 2))))

(defun add (a b) (+ a b))
"#;

/// The same module with exactly one defect introduced per rule.
///
/// - `classify` gets a `lists:member` call in a guard — a compile error.
/// - `lookup`'s `case` gets a clause after its catch-all — a compiler warning.
const DANGEROUS: &str = r#"
(defmodule store
  (export (classify 1) (lookup 2)))

(defun lookup (key table)
  (case (dict:find key table)
    ((tuple 'ok value) value)
    (anything 'fallback)
    ('error 'not-found)))

(defun classify
  ((x) (when (lists:member x '(1 2 3))) 'small)
  ((_) 'unknown))
"#;

fn tree(source: &str) -> SyntaxTree {
    SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("the fixture must parse")
}

// -- the correct half ---------------------------------------------------------

/// Realistic, correct LFE produces nothing — through the real engine, so the
/// dialect gate and head index are exercised too.
#[test]
fn correct_lfe_yields_no_findings() {
    assert_eq!(
        fired(CORRECT, Dialect::Lfe),
        Vec::<String>::new(),
        "correct LFE must be silent"
    );
}

/// …and it is silent because the code is correct, not because there was
/// nothing to look at. Both denominators are non-zero.
#[test]
fn correct_lfe_still_has_candidates() {
    let tree = tree(CORRECT);
    let guards = illegal_guard_call::domain::candidate_count(Dialect::Lfe, &tree);
    let clauses = dead_clause::domain::candidate_count(Dialect::Lfe, &tree);
    assert!(
        guards >= 3,
        "the fixture must contain qualified guard calls for a clean sweep to mean anything, \
         found {guards}"
    );
    assert!(
        clauses >= 10,
        "the fixture must contain clause lists for a clean sweep to mean anything, found \
         {clauses}"
    );
}

// -- the dangerous twin -------------------------------------------------------

/// Each rule fires exactly once on the twin — not zero (the rule works) and
/// not more than once (it is not double-reporting).
#[test]
fn the_dangerous_twin_fires_each_rule_exactly_once() {
    let mut names = fired(DANGEROUS, Dialect::Lfe);
    names.sort();
    assert_eq!(
        names,
        vec!["lfe-clause-after-catch-all", "lfe-illegal-guard-call"],
        "each rule must fire exactly once"
    );
}

/// The twin differs from the correct fixture only in the two defects, so the
/// rules are responding to those and not to some other edit.
#[test]
fn the_twin_findings_point_at_the_introduced_defects() {
    let tree = tree(DANGEROUS);
    let guards = illegal_guard_call::domain::collect(Dialect::Lfe, &tree);
    assert_eq!(guards.len(), 1);
    assert_eq!(guards[0].module, "lists");
    assert_eq!(guards[0].function, "member");

    let clauses = dead_clause::domain::collect(Dialect::Lfe, &tree);
    assert_eq!(clauses.len(), 1);
    assert_eq!(clauses[0].form, dead_clause::domain::ClauseForm::Case);
    assert_eq!(
        clauses[0].span.slice(DANGEROUS),
        "('error 'not-found)",
        "the reported clause is the unreachable one, not the catch-all"
    );
}

// -- the fixtures themselves --------------------------------------------------

/// A fixture that stopped parsing would make every assertion above vacuous.
#[test]
fn both_fixtures_parse() {
    assert!(!tree(CORRECT).root_view().children.is_empty());
    assert!(!tree(DANGEROUS).root_view().children.is_empty());
}

/// The correct fixture must actually contain the constructs it claims to,
/// or a later edit could quietly hollow it out.
#[test]
fn the_correct_fixture_covers_every_construct() {
    for needle in [
        "(defsyntax",
        "(receive",
        "(after ",
        "(call 'erlang",
        "(: erlang",
        "erlang:is_atom",
        "`(case",
        "'(when",
        "(defun add (a b)",
    ] {
        assert!(
            CORRECT.contains(needle),
            "the correct fixture must still contain {needle}"
        );
    }
}
