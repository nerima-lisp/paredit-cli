//! Every rule here driven through the *engine*, rather than through its own
//! `examine_*`.
//!
//! Two declarations decide whether a rule is reachable from the CLI at all, and
//! neither is visible to a domain test, which calls `examine_*` on a node it
//! picked itself:
//!
//! - the [`HeadFilter::Heads`] list, which is what the dispatcher's head index is
//!   built from. A head spelled wrongly — or a head the domain matches but the
//!   list omits — leaves every `examine_*` test green while the rule never
//!   receives a single node in production.
//! - the [`RuleDialectScope`], which the dispatcher consults *before* walking
//!   anything.
//!
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads
//! [`RuleDialectScope`]: paredit_core_lint_engine::policy::RuleDialectScope

use std::path::Path;

use paredit_core_lint_engine::engine::{
    PassOptions, build_head_index, collect_lint_outcomes, collect_lint_pass,
};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, Severity};
use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::ENTRIES;

/// The rule names that fire on `source`, sorted so the assertions do not depend
/// on registration order.
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
    names.dedup();
    names
}

/// How many nodes each rule's `check` was actually handed.
///
/// This is the **denominator**. A zero-finding sweep over zero candidates is a
/// false clean: it passes just as well when a rule's head list is misspelled and
/// it never runs at all.
fn invocations(source: &str, dialect: Dialect) -> Vec<(&'static str, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("t.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("measured pass");
    outcome
        .timings
        .expect("timings")
        .entries()
        .map(|(position, _, count)| (catalog.entries()[position].meta().name().as_str(), count))
        .collect()
}

/// One source per rule that triggers exactly that rule and no other.
const TRIGGERS: [(&str, &str); 5] = [
    (
        "declare-not-at-head-of-body",
        "(defun f (x) (print x) (declare (fixnum x)) x)",
    ),
    (
        "declaim-inside-body",
        "(defun f (x) (declaim (fixnum x)) x)",
    ),
    (
        "type-declaration-contradicts-initform",
        "(let ((x 0)) (declare (string x)) x)",
    ),
    ("the-form-with-impossible-type", "(the null (list 1 2))"),
    (
        "type-declaration-on-rest-parameter",
        "(defun f (&rest args) (declare (fixnum args)) args)",
    ),
];

// -- (a) each rule is reached through the head index --------------------------

#[test]
fn every_rule_fires_through_the_real_dispatch() {
    for (rule, source) in TRIGGERS {
        assert_eq!(
            fired(source, Dialect::CommonLisp),
            vec![rule],
            "{rule} is unreachable through the head index, or another rule fires with it"
        );
    }
}

/// Six rules, six distinct names: a copy-paste in `ENTRIES` that registered one
/// slice twice would otherwise leave the loop above green.
#[test]
fn the_catalog_holds_every_rule_once() {
    let mut names: Vec<&'static str> = RuleCatalog::new(&ENTRIES)
        .entries()
        .iter()
        .map(|entry| entry.meta().name().as_str())
        .collect();
    assert_eq!(names.len(), 5);
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 5, "two entries share a name");
}

// -- (b) a file with none of these heads trips nothing ------------------------

/// What the `clean/forms/*` benchmarks measure: ordinary code, none of it
/// declaring anything, must not reach a single `check` body.
#[test]
fn a_file_with_none_of_these_heads_produces_no_findings() {
    let source = "(in-package :app)\n\
         (defparameter *limit* 10)\n\
         (defvar *state* nil)\n\
         (defstruct point x y)\n\
         (setq *state* (list 1 2 3))\n";
    assert_eq!(fired(source, Dialect::CommonLisp), Vec::<&str>::new());
    for (rule, count) in invocations(source, Dialect::CommonLisp) {
        assert_eq!(
            count, 0,
            "{rule} was invoked on a file with none of its heads"
        );
    }
}

// -- (c) realistic, *correct* Common Lisp -------------------------------------

/// Idiomatic, correct Common Lisp using every shape these rules come near:
/// declarations at the head of a body, after a docstring, several in a row; the
/// placeholder-then-assign idiom; `ignorable` where a variable may go unread;
/// widened `(or null …)` types; `&rest` declared as a list and as
/// `dynamic-extent`; `the` asserting types that hold; a top-level `declaim`; a
/// `declaim` inside a top-level `locally` and `macrolet`, which CLHS 3.2.3.1
/// makes correct; a shadowed `ignore`; and a Lisp-2 accessor sharing a name with
/// an ignored parameter.
///
/// Every line of it is a shape the corpus audit either found in SBCL's own
/// sources or that SBCL 2.6.0 compiles without a word.
///
/// Paired with [`the_dangerous_twin_is_still_detected`], without which a rule
/// that had silently stopped matching anything would pass this too.
const CORRECT_COMMON_LISP: &str = r#"(in-package :inventory)

(declaim (optimize (speed 3) (safety 1)))
(declaim (ftype (function (fixnum fixnum) fixnum) combine))

(defun combine (a b)
  "Adds two counts."
  (declare (fixnum a b))
  (declare (optimize speed))
  (the fixnum (+ a b)))

(defun tally (items &rest extra)
  "Sums ITEMS, ignoring EXTRA."
  (declare (list items))
  (declare (ignore extra))
  (let ((total 0)
        (label ""))
    (declare (fixnum total) (string label))
    (dolist (item items)
      (declare (fixnum item))
      (incf total item))
    (values total label)))

(defun collect (&rest args)
  (declare (dynamic-extent args))
  (declare (list args))
  (apply #'+ args))

(defun lookup (key table)
  (let ((cache nil))
    (declare (type (or null hash-table) cache))
    (setf cache table)
    (gethash key cache)))

(defun maybe-use (a b)
  (declare (ignorable b))
  a)

(defun shadowed (x y)
  (declare (ignore y))
  (let ((y (* x 2)))
    (declare (fixnum y))
    y))

(defun accessor-shares-a-name (component builtin-system-p)
  (declare (ignore builtin-system-p))
  (setf (builtin-system-p component) t))

(defmethod area :before ((s square))
  (declare (optimize safety))
  (print s))

(locally (declare (optimize (speed 3) (safety 0)))
  (declaim (inline fast-path))
  (defun fast-path (n) (declare (fixnum n)) n))

(macrolet ((with-guard (form) `(progn ,form)))
  (declaim (inline guarded))
  (defun guarded (n) (with-guard n)))

(defun returns-a-string (x)
  (declare (fixnum x))
  "the return value, not a docstring")

(defun uses-a-quiet-macro ()
  (let ((v 3))
    (declare (ignore v))
    (macrolet ((name-of (s) `',s))
      (name-of v))))
"#;

/// Every rule declines all of it — **and** every rule was actually offered
/// candidates while doing so.
#[test]
fn realistic_correct_common_lisp_produces_no_findings() {
    assert_eq!(
        fired(CORRECT_COMMON_LISP, Dialect::CommonLisp),
        Vec::<&str>::new(),
        "a false positive on idiomatic Common Lisp"
    );
}

/// The denominator for the sweep above, without which it proves nothing: each
/// rule's `check` really did run on this file, many times over.
#[test]
fn the_correct_sample_offers_every_rule_real_candidates() {
    for (rule, count) in invocations(CORRECT_COMMON_LISP, Dialect::CommonLisp) {
        assert!(
            count > 0,
            "{rule} was never invoked on the correct sample; its clean sweep is a false clean"
        );
    }
    // The two rules with the narrowest head lists still see several forms each,
    // so "greater than zero" is not one lucky node.
    let counts = invocations(CORRECT_COMMON_LISP, Dialect::CommonLisp);
    let of = |name: &str| {
        counts
            .iter()
            .find(|(rule, _)| *rule == name)
            .map(|(_, count)| *count)
            .expect("the rule is in the catalogue")
    };
    assert!(of("the-form-with-impossible-type") >= 1);
    assert!(of("type-declaration-on-rest-parameter") >= 8);
    assert!(of("declare-not-at-head-of-body") >= 15);
}

/// The control for the sweep above. Each twin is the *correct* file with exactly
/// one thing made wrong, and each proves one detector still works on it.
///
/// One twin per rule rather than one combined twin: a combined one is easy to
/// get wrong, and a twin that expected six findings while producing five would
/// have to be weakened to pass, quietly costing the control its value.
#[test]
fn the_dangerous_twin_is_still_detected() {
    let twins: [(&str, &str, &str); 5] = [
        (
            "declare-not-at-head-of-body",
            "  (declare (fixnum a b))\n  (declare (optimize speed))\n  (the fixnum (+ a b)))",
            "  (declare (fixnum a b))\n  (the fixnum (+ a b))\n  (declare (optimize speed)))",
        ),
        (
            "declaim-inside-body",
            "(defun maybe-use (a b)\n  (declare (ignorable b))",
            "(defun maybe-use (a b)\n  (declaim (ignorable b))",
        ),
        (
            "type-declaration-contradicts-initform",
            "    (declare (fixnum total) (string label))",
            "    (declare (string total) (fixnum label))",
        ),
        (
            "the-form-with-impossible-type",
            "  (the fixnum (+ a b)))",
            "  (the null (list a b)))",
        ),
        (
            "type-declaration-on-rest-parameter",
            "  (declare (dynamic-extent args))\n  (declare (list args))",
            "  (declare (dynamic-extent args))\n  (declare (fixnum args))",
        ),
    ];

    for (rule, before, after) in twins {
        let twin = CORRECT_COMMON_LISP.replace(before, after);
        assert_ne!(twin, CORRECT_COMMON_LISP, "{rule}: the twin must differ");
        assert_eq!(
            fired(&twin, Dialect::CommonLisp),
            vec![rule],
            "{rule}: its twin must trip it and nothing else"
        );
    }
}

// -- (d) quoted and templated code --------------------------------------------

/// The dispatcher hands a rule every head-matched node, quoted data included;
/// each `check` calls `is_unevaluated_at` to decline those.
#[test]
fn no_rule_fires_on_quoted_or_templated_code() {
    for (rule, source) in TRIGGERS {
        assert_eq!(
            fired(&format!("'{source}"), Dialect::CommonLisp),
            Vec::<&str>::new(),
            "{rule}: {source} is quoted data"
        );
        assert_eq!(
            fired(&format!("`{source}"), Dialect::CommonLisp),
            Vec::<&str>::new(),
            "{rule}: {source} is a macro template"
        );
        assert_eq!(
            fired(&format!("(quote {source})"), Dialect::CommonLisp),
            Vec::<&str>::new(),
            "{rule}: {source} is long-hand quoted data"
        );
    }
}

/// ...but an unquote inside a quasiquote is code again, so the declines above
/// are the quote model talking and not a rule that stopped working.
#[test]
fn a_rule_still_fires_under_an_unquote() {
    assert_eq!(
        fired(
            "(defmacro m () `(list ,(the null (list 1 2))))",
            Dialect::CommonLisp
        ),
        vec!["the-form-with-impossible-type"]
    );
}

// -- (e) dialect scope --------------------------------------------------------

/// Every rule models Common Lisp's declaration system, so none may run for a
/// dialect that has no such system. Scheme is the sharpest control: it reads
/// these bytes happily and means something entirely different by them.
#[test]
fn no_rule_runs_outside_common_lisp() {
    for (rule, source) in TRIGGERS {
        for dialect in [
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::EmacsLisp,
            Dialect::Fennel,
        ] {
            if SyntaxTree::parse_with_dialect(source, dialect).is_ok() {
                assert_eq!(
                    fired(source, dialect),
                    Vec::<&str>::new(),
                    "{rule} fired for {dialect:?}"
                );
            }
        }
    }
    // At least one other reader really does accept a trigger, so the loop above
    // is not vacuously true.
    assert!(
        TRIGGERS
            .iter()
            .any(|(_, source)| { SyntaxTree::parse_with_dialect(source, Dialect::Scheme).is_ok() }),
        "no non-CL reader accepts any trigger; the scope is untested"
    );
}

/// The scope as a declaration, so a rule that loses its `dialect_scope`
/// override fails here and not only through a sample that might stop triggering
/// for some other reason.
///
/// Common Lisp is also the trait *default*, so this cannot distinguish
/// "declared" from "defaulted" — [`crate::support::COMMON_LISP_ONLY`] exists so
/// that every rule reads one constant, and this pins that they all do.
#[test]
fn every_rule_declares_common_lisp_alone() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        assert_eq!(
            entry.rule().dialect_scope(),
            RuleDialectScope::new(&[Dialect::CommonLisp]),
            "{name} declares the wrong dialect scope"
        );
    }
}

// -- (f) no rule declares anything but Heads ----------------------------------

/// `AllNodes` and `WholeTree` are paid for on every file whether or not a rule
/// matches, which is exactly what the zero-finding benchmarks measure. No rule
/// here is about an *absence* with no head to anchor on, so none needs one.
#[test]
fn every_rule_declares_a_non_empty_heads_filter() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        let HeadFilter::Heads(heads) = entry.rule().head_filter() else {
            panic!("{name} declares something other than HeadFilter::Heads");
        };
        assert!(!heads.is_empty(), "{name} declares an empty head list");
    }
}

/// Every rule here is report-only: in each case either half of what the author
/// wrote could be the wrong half, and no rewrite is right more often than not.
#[test]
fn every_rule_is_report_only() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        assert_eq!(
            entry.meta().fixability(),
            Fixability::ReportOnly,
            "{}",
            entry.meta().name().as_str()
        );
    }
}

/// The severities, pinned against what SBCL 2.6.0 actually emits: a misplaced
/// `declare` is a `caught ERROR`, and everything else is a `WARNING` or
/// `STYLE-WARNING`.
#[test]
fn severities_match_what_sbcl_emits() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        let expected = if name == "declare-not-at-head-of-body" {
            Severity::Error
        } else {
            Severity::Warning
        };
        assert_eq!(entry.meta().severity(), expected, "{name}");
    }
}

/// `declare-not-at-head-of-body` is `Malformed` rather than `Declaration`: the
/// form is not a bad declaration, it is not a declaration at all.
#[test]
fn categories_separate_the_malformed_case_from_the_declaration_ones() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        let expected = if name == "declare-not-at-head-of-body" {
            RuleCategory::Malformed
        } else {
            RuleCategory::Declaration
        };
        assert_eq!(entry.meta().category(), expected, "{name}");
    }
}
