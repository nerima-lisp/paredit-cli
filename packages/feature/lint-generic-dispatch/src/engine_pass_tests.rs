//! Every rule here driven through the *engine*, rather than through its own
//! `examine_*`.
//!
//! Two declarations decide whether a rule is reachable at all, and neither is
//! visible to a domain test, which calls `examine_*` on a node it picked itself:
//!
//! - the `HeadFilter::Heads` list, which is what the dispatcher's head index is
//!   built from. A head spelled wrongly — or a head the domain matches but the
//!   list omits — leaves every `examine_*` test green while the rule never
//!   receives a single node in production;
//! - the `RuleDialectScope`, which the dispatcher consults *before* walking
//!   anything.

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
///
/// `pub(crate)` because `cost_tests` asserts that its "clean" cost fixture
/// really is clean — a cost number measured on a file that reports is the cost
/// of reporting, not the cost of declining.
pub(crate) fn fired_names(source: &str, dialect: Dialect) -> Vec<&'static str> {
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

fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
    fired_names(source, dialect)
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
const TRIGGERS: [(&str, &str); 3] = [
    (
        "defgeneric-method-option-incongruent",
        "(defgeneric draw (shape)\n  (:method ((s circle) stream) s))",
    ),
    (
        "initialization-primary-without-call-next-method",
        "(defmethod initialize-instance ((o widget) &key) (setup o))",
    ),
    (
        "class-allocated-slot-with-initarg",
        "(defclass registry () ((entries :initarg :entries :allocation :class)))",
    ),
];

// -- (a) each rule is reached through the head index ---------------------------

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

/// Three rules, three distinct names: a copy-paste in `ENTRIES` that registered
/// one slice twice would otherwise leave the loop above green.
#[test]
fn the_catalog_holds_every_rule_once() {
    let mut names: Vec<&'static str> = RuleCatalog::new(&ENTRIES)
        .entries()
        .iter()
        .map(|entry| entry.meta().name().as_str())
        .collect();
    assert_eq!(names.len(), 3);
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 3, "two entries share a name");
}

// -- (b) a file with none of these heads trips nothing -------------------------

#[test]
fn a_file_with_none_of_these_heads_produces_no_findings() {
    let source = "(in-package :app)\n\
         (defparameter *limit* 10)\n\
         (defun combine (a b) (+ a b))\n\
         (defstruct point x y)\n\
         (defvar *state* nil)\n";
    assert_eq!(fired(source, Dialect::CommonLisp), Vec::<&str>::new());
    for (rule, count) in invocations(source, Dialect::CommonLisp) {
        assert_eq!(
            count, 0,
            "{rule} was invoked on a file with none of its heads"
        );
    }
}

// -- (c) realistic, *correct* CLOS ---------------------------------------------

/// Idiomatic, correct Common Lisp using every shape these rules come near.
///
/// Congruent methods of every kind: qualified, `&key`-taking, keyword-adding,
/// `&rest`-against-`&key`, `&allow-other-keys`; a `defgeneric` carrying its own
/// `(:method …)` default; the `initialize-instance :after` idiom; an `:around`
/// that deliberately short-circuits, which belongs to another package's rule; a
/// `shared-initialize` primary that does chain; a class slot with no initarg and
/// an instance slot with one; a method whose `defgeneric` is in another file; a
/// `(setf …)` generic; a `defclass` reaching a class slot through
/// `:default-initargs`; and a macro template full of `defmethod`s.
///
/// Every line of it is a shape SBCL 2.6.0 compiles and runs without a word, or
/// that the corpus audit found in third-party sources.
///
/// Paired with [`the_dangerous_twin_is_still_detected`], without which a rule
/// that had silently stopped matching anything would pass this too.
const CORRECT_CLOS: &str = r#"(in-package :shapes)

(defclass shape ()
  ((origin :initarg :origin :initform (list 0 0) :accessor origin-of)
   (instances :allocation :class :initform 0 :accessor instance-count))
  (:documentation "A drawable thing."))

(defclass circle (shape)
  ((radius :initarg :radius :initform 1 :accessor radius-of)))

(defclass registry ()
  ((entries :allocation :class :initform nil :accessor entries-of))
  (:default-initargs :name "default"))

(defgeneric draw (shape stream)
  (:documentation "Draws SHAPE on STREAM.")
  (:method ((s shape) stream) (format stream "~a" s)))

(defmethod draw ((s circle) stream)
  (format stream "circle ~a" (radius-of s)))

(defmethod draw :around ((s circle) stream)
  (if (zerop (radius-of s))
      :nothing-to-draw
      (call-next-method)))

(defmethod draw :before ((s shape) stream)
  (setf (instance-count s) (1+ (instance-count s))))

(defgeneric scale (shape factor &key clamp))

(defmethod scale ((s circle) factor &key clamp)
  (setf (radius-of s) (if clamp (min factor 10) factor)))

(defmethod scale ((s shape) factor &key clamp origin)
  (declare (ignore clamp origin factor))
  s)

(defgeneric render (shape &rest options))

(defmethod render ((s circle) &key colour)
  (list s colour))

(defgeneric describe-shape (shape &key verbose))

(defmethod describe-shape ((s shape) &rest ignored)
  (declare (ignore ignored))
  s)

(defmethod describe-shape ((s circle) &key &allow-other-keys)
  s)

(defgeneric (setf width) (value shape))

(defmethod (setf width) (value (s circle))
  (setf (radius-of s) (/ value 2)))

(defmethod initialize-instance :after ((s circle) &key radius)
  (when radius (setf (radius-of s) radius)))

(defmethod shared-initialize ((s shape) slots &rest initargs &key)
  (declare (ignore initargs slots))
  (call-next-method))

(defmethod reinitialize-instance :before ((s shape) &key)
  (setf (origin-of s) (list 0 0)))

(defmethod print-object ((s circle) stream)
  (print-unreadable-object (s stream :type t)
    (format stream "~a" (radius-of s))))

(defmethod area ((s circle))
  (* pi (radius-of s) (radius-of s)))

(defmacro define-trivial-shape (name)
  `(progn
     (defclass ,name (shape) ())
     (defmethod draw ((s ,name) stream (extra t)) stream)
     (defmethod initialize-instance ((s ,name) &key) s)))
"#;

/// Every rule declines all of it.
#[test]
fn realistic_correct_clos_produces_no_findings() {
    assert_eq!(
        fired(CORRECT_CLOS, Dialect::CommonLisp),
        Vec::<&str>::new(),
        "a false positive on idiomatic CLOS"
    );
}

/// The denominator for the sweep above, without which it proves nothing: each
/// rule's `check` really did run on this file, many times over.
#[test]
fn the_correct_sample_offers_every_rule_real_candidates() {
    let counts = invocations(CORRECT_CLOS, Dialect::CommonLisp);
    for (rule, count) in &counts {
        assert!(
            *count > 0,
            "{rule} was never invoked on the correct sample; its clean sweep is a false clean"
        );
    }
    let of = |name: &str| {
        counts
            .iter()
            .find(|(rule, _)| *rule == name)
            .map(|(_, count)| *count)
            .expect("the rule is in the catalogue")
    };
    // "Greater than zero" must not be one lucky node.
    assert!(
        of("defgeneric-method-option-incongruent") >= 5,
        "only {} defgeneric candidates",
        of("defgeneric-method-option-incongruent")
    );
    assert!(
        of("initialization-primary-without-call-next-method") >= 15,
        "only {} defmethod candidates",
        of("initialization-primary-without-call-next-method")
    );
    assert!(
        of("class-allocated-slot-with-initarg") >= 3,
        "only {} defclass candidates",
        of("class-allocated-slot-with-initarg")
    );
}

/// The control for the sweep above. Each twin is the *correct* file with exactly
/// one thing made wrong, and each proves one detector still works on it.
///
/// One twin per rule rather than one combined twin: a combined one is easy to
/// get wrong, and a twin that expected three findings while producing two would
/// have to be weakened to pass, quietly costing the control its value.
#[test]
fn the_dangerous_twin_is_still_detected() {
    let twins: [(&str, &str, &str); 3] = [
        (
            "defgeneric-method-option-incongruent",
            "  (:method ((s shape) stream) (format stream \"~a\" s)))",
            "  (:method ((s shape)) (format t \"~a\" s)))",
        ),
        (
            "initialization-primary-without-call-next-method",
            "(defmethod initialize-instance :after ((s circle) &key radius)",
            "(defmethod initialize-instance ((s circle) &key radius)",
        ),
        (
            "class-allocated-slot-with-initarg",
            "(instances :allocation :class :initform 0 :accessor instance-count)",
            "(instances :allocation :class :initarg :instances :accessor instance-count)",
        ),
    ];

    for (rule, before, after) in twins {
        let twin = CORRECT_CLOS.replace(before, after);
        assert_ne!(twin, CORRECT_CLOS, "{rule}: the twin must differ");
        assert_eq!(
            fired(&twin, Dialect::CommonLisp),
            vec![rule],
            "{rule}: its twin must trip it and nothing else"
        );
    }
}

// -- (d) quoted and templated code ---------------------------------------------

/// The dispatcher hands a rule every head-matched node, quoted data included;
/// each `check` calls `is_unevaluated_at` to decline those.
#[test]
fn no_rule_fires_on_quoted_or_templated_code() {
    for (rule, source) in TRIGGERS {
        for wrapped in [
            format!("'({source})"),
            format!("`({source})"),
            format!("(quote ({source}))"),
        ] {
            assert_eq!(
                fired(&wrapped, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{rule}: {wrapped} is unevaluated data"
            );
        }
    }
}

/// ...but an unquote inside a quasiquote is code again, so the declines above
/// are the quote model talking and not a rule that stopped working.
#[test]
fn a_rule_still_fires_under_an_unquote() {
    assert_eq!(
        fired(
            "(defmacro m () `(list ,(defclass r () ((e :initarg :e :allocation :class)))))",
            Dialect::CommonLisp
        ),
        vec!["class-allocated-slot-with-initarg"]
    );
}

// -- (e) dialect scope ---------------------------------------------------------

/// Every rule models the CLOS dispatch protocol, so none may run for a dialect
/// that has no such protocol. Clojure is the sharpest control: it has a
/// `defmethod` of its own that means something entirely different.
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
    assert!(
        TRIGGERS
            .iter()
            .any(|(_, source)| SyntaxTree::parse_with_dialect(source, Dialect::Clojure).is_ok()),
        "no non-CL reader accepts any trigger; the scope is untested"
    );
}

/// The scope as a declaration, so a rule that loses its `dialect_scope` override
/// fails here and not only through a sample that might stop triggering.
#[test]
fn every_rule_declares_common_lisp_alone() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        assert_eq!(
            entry.rule().dialect_scope(),
            RuleDialectScope::new(&[Dialect::CommonLisp]),
            "{} declares the wrong dialect scope",
            entry.meta().name().as_str()
        );
    }
}

// -- (f) declarations ----------------------------------------------------------

/// `WholeTree` and `AllNodes` are paid for on every file whether or not a rule
/// matches, which is exactly what the CI benchmark gate measures. No rule here
/// is about an absence with no head to anchor on, so none needs one.
#[test]
fn every_rule_declares_a_non_empty_heads_filter() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        let HeadFilter::Heads(heads) = entry.rule().head_filter() else {
            panic!("{name} declares something other than HeadFilter::Heads");
        };
        assert!(!heads.is_empty(), "{name} declares an empty head list");
        for head in heads {
            assert!(
                ["defgeneric", "defmethod", "defclass"].contains(&head.as_str()),
                "{name} anchors on an unexpected head {}",
                head.as_str()
            );
        }
    }
}

/// Every rule is report-only: in each case which half of what the author wrote
/// is the wrong half *is* the finding, and no rewrite is right more often than
/// not.
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

/// The severities, pinned against what SBCL 2.6.0 actually does.
///
/// The two `Error`s are the cases where the program is provably broken:
/// congruence makes `defmethod` signal `SIMPLE-PROGRAM-ERROR`, and a primary
/// initialization method that never chains hands back an instance with every
/// slot unbound. The `Warning` is the case where the program is conforming and
/// nothing signals — two slot options merely contradict each other.
#[test]
fn severities_match_what_sbcl_does() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        let expected = if name == "class-allocated-slot-with-initarg" {
            Severity::Warning
        } else {
            Severity::Error
        };
        assert_eq!(entry.meta().severity(), expected, "{name}");
    }
}

/// Every rule here is about CLOS's `defgeneric`/`defmethod`/`defclass`
/// agreement, which is exactly what `ObjectSystem` names.
#[test]
fn every_rule_is_in_the_object_system_category() {
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        assert_eq!(
            entry.meta().category(),
            RuleCategory::ObjectSystem,
            "{}",
            entry.meta().name().as_str()
        );
    }
}
