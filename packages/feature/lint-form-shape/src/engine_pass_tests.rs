//! The eight new rules through the real dispatch, plus the corpus sweep.
//!
//! See the module's doc comment in `lib.rs` for why the report path is not
//! enough on its own.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

static ENTRIES: [RuleEntry; 8] = [
    RuleEntry::new(
        &crate::destructuring_bind_unused_whole::rule::META,
        &crate::destructuring_bind_unused_whole::rule::RULE,
    ),
    RuleEntry::new(
        &crate::flet_single_use_inlinable::rule::META,
        &crate::flet_single_use_inlinable::rule::RULE,
    ),
    RuleEntry::new(
        &crate::ftype_values_arity_mismatch::rule::META,
        &crate::ftype_values_arity_mismatch::rule::RULE,
    ),
    RuleEntry::new(
        &crate::loop_collect_into_immediately_returned::rule::META,
        &crate::loop_collect_into_immediately_returned::rule::RULE,
    ),
    RuleEntry::new(
        &crate::multiple_value_setq_arity_mismatch::rule::META,
        &crate::multiple_value_setq_arity_mismatch::rule::RULE,
    ),
    RuleEntry::new(
        &crate::quoted_form_contains_stray_unquote::rule::META,
        &crate::quoted_form_contains_stray_unquote::rule::RULE,
    ),
    RuleEntry::new(
        &crate::with_accessors_empty_binding_list::rule::META,
        &crate::with_accessors_empty_binding_list::rule::RULE,
    ),
    RuleEntry::new(
        &crate::with_open_file_redundant_direction_default::rule::META,
        &crate::with_open_file_redundant_direction_default::rule::RULE,
    ),
];

/// Every rule name this batch adds, sorted.
const RULE_NAMES: [&str; 8] = [
    "destructuring-bind-unused-whole",
    "flet-single-use-inlinable",
    "ftype-values-arity-mismatch",
    "loop-collect-into-immediately-returned",
    "multiple-value-setq-arity-mismatch",
    "quoted-form-contains-stray-unquote",
    "with-accessors-empty-binding-list",
    "with-open-file-redundant-direction-default",
];

/// The rule names that fire on `source`, sorted so the assertions do not
/// depend on registration order. Duplicates are kept: a rule that fires twice
/// appears twice.
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

/// The distinct rule names that fire on `source`.
fn fired_distinct(source: &str, dialect: Dialect) -> Vec<&'static str> {
    let mut names = fired(source, dialect);
    names.dedup();
    names
}

// ---------------------------------------------------------------------------
// Each rule reaches the engine
// ---------------------------------------------------------------------------

#[test]
fn every_rule_fires_through_the_real_dispatch() {
    assert_eq!(
        fired(
            "(destructuring-bind (&whole all p q) x (list p q))",
            Dialect::CommonLisp
        ),
        vec!["destructuring-bind-unused-whole"]
    );
    assert_eq!(
        fired(
            "(flet ((double (y) (* y 2))) (double n))",
            Dialect::CommonLisp
        ),
        vec!["flet-single-use-inlinable"]
    );
    assert_eq!(
        fired(
            "(declaim (ftype (function (integer) (values integer integer)) f))\n\
             (defun f (x) (values x))\n",
            Dialect::CommonLisp
        ),
        vec!["ftype-values-arity-mismatch"]
    );
    assert_eq!(
        fired(
            "(loop for x in items collect x into acc finally (return acc))",
            Dialect::CommonLisp
        ),
        vec!["loop-collect-into-immediately-returned"]
    );
    assert_eq!(
        fired(
            "(multiple-value-setq (a b c) (values 1 2))",
            Dialect::CommonLisp
        ),
        vec!["multiple-value-setq-arity-mismatch"]
    );
    assert_eq!(
        fired("(defmacro m (x) '(f ,x))", Dialect::CommonLisp),
        vec!["quoted-form-contains-stray-unquote"]
    );
    assert_eq!(
        fired("(with-slots () obj (frob))", Dialect::CommonLisp),
        vec!["with-accessors-empty-binding-list"]
    );
    assert_eq!(
        fired(
            "(with-open-file (s p :direction :input) (read-line s))",
            Dialect::CommonLisp
        ),
        vec!["with-open-file-redundant-direction-default"]
    );
}

/// Every name this batch claims is in fact the name the engine reports, so a
/// typo in a `RuleMeta` cannot pass unnoticed.
#[test]
fn the_catalogue_names_match_the_names_the_engine_reports() {
    let mut declared: Vec<&str> = ENTRIES
        .iter()
        .map(|entry| entry.meta().name().as_str())
        .collect();
    declared.sort_unstable();
    assert_eq!(declared, RULE_NAMES.to_vec());
}

// ---------------------------------------------------------------------------
// The guard the report path cannot exercise
// ---------------------------------------------------------------------------

/// The dispatcher hands a rule every head-matched node, quoted or not. Without
/// each `check`'s `is_unevaluated_at` call, every one of these fires.
///
/// `quoted-form-contains-stray-unquote` is in the list too: its polarity is
/// inverted with respect to the *comma*, not with respect to the matched form,
/// and a `defmacro` inside `'(…)` is a list of symbols either way.
#[test]
fn no_rule_fires_on_a_hard_quoted_form() {
    for source in [
        "'(destructuring-bind (&whole all p q) x (list p q))",
        "'(flet ((double (y) (* y 2))) (double n))",
        "'(loop for x in items collect x into acc finally (return acc))",
        "'(multiple-value-setq (a b c) (values 1 2))",
        "'(defmacro m (x) '(f ,x))",
        "'(with-slots () obj (frob))",
        "'(with-open-file (s p :direction :input) (read-line s))",
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
        "(quote (with-slots () obj (frob)))",
        "(quote (multiple-value-setq (a b) (values 1)))",
        "(quote (flet ((double (y) (* y 2))) (double n)))",
    ] {
        assert_eq!(
            fired(source, Dialect::CommonLisp),
            Vec::<&str>::new(),
            "{source}"
        );
    }
}

/// A macro template: the form is built, not run.
#[test]
fn no_rule_fires_inside_a_quasiquoted_macro_template() {
    assert_eq!(
        fired(
            "(defmacro m (o) `(with-slots () ,o (frob)))",
            Dialect::CommonLisp
        ),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired(
            "(defmacro m (p) `(with-open-file (s ,p :direction :input) (read-line s)))",
            Dialect::CommonLisp
        ),
        Vec::<&str>::new()
    );
}

/// A comma inside a hard quote is a literal comma, not an escape — the shape a
/// single depth counter reads wrongly. Every rule but one must be silent, and
/// the one exception must be *loud*, which is the whole point of the inverted
/// polarity.
#[test]
fn a_comma_inside_a_hard_quote_silences_every_rule_but_the_one_that_is_about_it() {
    assert_eq!(
        fired("'(a ,(with-slots () obj (frob)))", Dialect::CommonLisp),
        Vec::<&str>::new()
    );
    // The same shape, reached through a head this batch anchors on: the comma
    // is the finding.
    assert_eq!(
        fired("(quote (a ,x))", Dialect::CommonLisp),
        vec!["quoted-form-contains-stray-unquote"]
    );
}

/// The one shape that *is* code again — and, for the stray-unquote rule, the
/// one shape that must stay silent.
#[test]
fn an_unquote_inside_a_quasiquote_still_fires_for_the_ordinary_rules() {
    assert_eq!(
        fired("`(a ,(with-slots () obj (frob)))", Dialect::CommonLisp),
        vec!["with-accessors-empty-binding-list"]
    );
    assert_eq!(
        fired("(defmacro m (x) `(f ,x))", Dialect::CommonLisp),
        Vec::<&str>::new()
    );
}

// ---------------------------------------------------------------------------
// The declarations a domain test cannot see
// ---------------------------------------------------------------------------

/// `RuleDialectScope`: the dispatcher skips a rule before walking anything.
#[test]
fn no_rule_runs_outside_common_lisp() {
    for dialect in [
        Dialect::Clojure,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Fennel,
    ] {
        assert_eq!(
            fired("(with-slots () obj (frob))", dialect),
            Vec::<&str>::new(),
            "{dialect:?}"
        );
    }
}

/// `HeadFilter::Heads`: a file with none of this batch's heads is never handed
/// to any of these rules, which is what keeps the zero-finding benchmarks
/// cheap.
#[test]
fn no_rule_sees_a_form_that_is_none_of_its_heads() {
    let source = "(defun add (a b) (+ a b))\n\
                  (defvar *state* nil)\n\
                  (defclass thing () ((v :initarg :v)))\n\
                  (defmethod frob ((x thing)) (slot-value x 'v))\n\
                  (let ((x 1)) (incf x))\n\
                  (dolist (x '(1 2 3)) (print x))\n\
                  (case k ((:a) 1) (t 2))\n";
    assert_eq!(fired(source, Dialect::CommonLisp), Vec::<&str>::new());
}

// ---------------------------------------------------------------------------
// The corpus sweep
// ---------------------------------------------------------------------------

/// Realistic, correct Common Lisp that contains a *candidate* for every one of
/// the eight rules and a finding for none of them.
///
/// The point of the file is the denominators, not the zero: a corpus with no
/// instances of what a rule looks for proves nothing about that rule, which is
/// why [`the_correct_corpus_exercises_every_rule`] asserts each rule's own
/// candidate count is non-zero rather than only asserting the sweep is clean.
const CORRECT_CORPUS: &str = r#"(in-package :app/storage)

;; A &whole that is read, from a body and from a template.
(defun parse-entry (form)
  (destructuring-bind (&whole whole tag &rest payload) form
    (unless (keywordp tag)
      (error "~S is not a tagged entry" whole))
    (list tag payload)))

;; A collect ... into whose accumulator the loop body itself reads.
(defun take-until-full (items limit)
  (loop for item in items
        collect item into taken
        when (>= (length taken) limit)
          do (return taken)
        finally (return taken)))

;; A local function used twice, so naming it earns its keep.
(defun normalize-pair (a b)
  (flet ((scrub (value) (string-trim " " (string value))))
    (cons (scrub a) (scrub b))))

;; A multiple-value-setq whose arity agrees with its literal (values ...).
(defun split-clock (total)
  (let (minutes seconds)
    (multiple-value-setq (minutes seconds) (values (floor total 60) (mod total 60)))
    (list minutes seconds)))

;; An explicit :direction that is not the default.
(defun write-report (path text)
  (with-open-file (stream path :direction :output :if-exists :supersede)
    (write-string text stream)))

;; A declaimed ftype whose defun returns exactly what it promises.
(declaim (ftype (function (integer integer) (values integer integer)) divide-with-remainder))
(defun divide-with-remainder (numerator denominator)
  (values (floor numerator denominator) (mod numerator denominator)))

;; A with-slots that actually binds something.
(defun describe-thing (thing)
  (with-slots (name size) thing
    (format nil "~A (~D)" name size)))

;; A macro whose template is spelled with a backquote, including the ',v idiom
;; and a nested quoted list carrying a comma.
(defmacro define-tag (name value)
  `(progn
     (defparameter ,name ',value)
     (push '(tag ,value) *known-tags*)))

;; --- adversarially correct: the shapes most likely to be false positives ---

;; A &whole the author explicitly declared they do not use. The declaration is
;; a reference, so this must stay silent rather than repeat what it says.
(defun ignore-whole (form)
  (destructuring-bind (&whole whole tag) form
    (declare (ignore whole))
    tag))

;; A `',tag inside a lambda inside a macro: three reader prefixes on one atom,
;; and the classic shape a node-local prefix check misreads.
(defmacro with-tags (tags &body body)
  (let ((quoted (mapcar (lambda (tag) `',tag) tags)))
    `(let ((*known-tags* (list ,@quoted)))
       ,@body)))

;; A doubly nested template: ,,x and ,@,@y are escapes, not strays.
(defmacro define-builder (name)
  `(defmacro ,name (x &body forms)
     `(progn (record ',,'x) ,@forms)))

;; A quoted association list used as data, with no comma anywhere.
(defparameter *codes* (quote ((:ok . 200) (:missing . 404))))

;; A loop whose `for` variable is named like an accumulator, and a second
;; accumulation, so neither the shape nor the occurrence guard may match.
(defun tally (rows)
  (loop for acc in rows
        collect (first acc) into names
        sum (second acc) into total
        finally (return (values names total))))

;; A labels whose local function is genuinely recursive.
(defun depth-of (tree)
  (labels ((walk (node) (if (atom node) 0 (1+ (reduce #'max (mapcar #'walk node))))))
    (walk tree)))

;; A local function handed to a higher-order function rather than called.
(defun scrub-all (values)
  (flet ((scrub (v) (string-trim " " (string v))))
    (mapcar #'scrub values)))

;; An ftype whose declared return arity is a range.
(declaim (ftype (function (list) (values t &optional t)) first-two))
(defun first-two (items)
  (values (first items) (second items)))

;; An open with a computed direction.
(defun open-either (path mode)
  (open path :direction mode :if-does-not-exist nil))

;; Commas that are not unquotes at all.
(defun punctuation-table ()
  (list #\, "a,b" (quote (comma-separated values))))
"#;

/// The same eight shapes, each written wrong: the "dangerous twin" that proves
/// the sweep's harness can still detect a finding at all.
const DEFECTIVE_TWIN: &str = r#"(in-package :app/storage)

(defun parse-entry (form)
  (destructuring-bind (&whole whole tag &rest payload) form
    (list tag payload)))

(defun take-all (items)
  (loop for item in items collect item into taken finally (return taken)))

(defun normalize-one (a)
  (flet ((scrub (value) (string-trim " " (string value))))
    (scrub a)))

(defun split-clock (total)
  (multiple-value-setq (minutes seconds extra) (values (floor total 60) (mod total 60))))

(defun read-report (path)
  (with-open-file (stream path :direction :input)
    (read-line stream)))

(declaim (ftype (function (integer integer) (values integer integer)) halve))
(defun halve (numerator denominator)
  (values (floor numerator denominator)))

(defun describe-thing (thing)
  (with-slots () thing
    (format nil "opaque")))

(defmacro define-tag (name value)
  '(defparameter ,name ,value))
"#;

/// Each rule's candidate count on `source`, read from that rule's own report
/// denominator — the number the rule *could* have reported on, not the number
/// it did.
fn candidate_counts(source: &str) -> Vec<(&'static str, u64)> {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let path = Path::new("corpus.lisp");
    let dialect = Dialect::CommonLisp;

    fn only<T>(report: &paredit_core_cli::report::FileFindings<T>) -> u64 {
        report
            .summary
            .first()
            .and_then(|(_, value)| value.as_u64())
            .expect("a denominator")
    }

    vec![
        (
            "destructuring-bind-unused-whole",
            only(
                &crate::destructuring_bind_unused_whole::domain::build_destructuring_bind_unused_whole_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
        (
            "flet-single-use-inlinable",
            only(
                &crate::flet_single_use_inlinable::domain::build_flet_single_use_inlinable_report(
                    path, dialect, &tree,
                )
                .expect("report"),
            ),
        ),
        (
            "ftype-values-arity-mismatch",
            only(
                &crate::ftype_values_arity_mismatch::domain::build_ftype_values_arity_mismatch_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
        (
            "loop-collect-into-immediately-returned",
            only(
                &crate::loop_collect_into_immediately_returned::domain::build_loop_collect_into_immediately_returned_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
        (
            "multiple-value-setq-arity-mismatch",
            only(
                &crate::multiple_value_setq_arity_mismatch::domain::build_multiple_value_setq_arity_mismatch_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
        (
            "quoted-form-contains-stray-unquote",
            only(
                &crate::quoted_form_contains_stray_unquote::domain::build_quoted_form_contains_stray_unquote_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
        (
            "with-accessors-empty-binding-list",
            only(
                &crate::with_accessors_empty_binding_list::domain::build_with_accessors_empty_binding_list_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
        (
            "with-open-file-redundant-direction-default",
            only(
                &crate::with_open_file_redundant_direction_default::domain::build_with_open_file_redundant_direction_default_report(path, dialect, &tree)
                    .expect("report"),
            ),
        ),
    ]
}

/// The corpus must actually contain something each rule looks at. A sweep over
/// a file with no candidates is a sweep that proves nothing, which is exactly
/// how a previous batch shipped three rules its corpus never touched.
#[test]
fn the_correct_corpus_exercises_every_rule() {
    for (rule, count) in candidate_counts(CORRECT_CORPUS) {
        assert!(
            count > 0,
            "{rule} has no candidate in the correct corpus, so the sweep says nothing about it"
        );
    }
}

/// …and says nothing about it. This is the false-positive gate.
#[test]
fn the_correct_corpus_produces_no_findings() {
    assert_eq!(
        fired(CORRECT_CORPUS, Dialect::CommonLisp),
        Vec::<&str>::new()
    );
}

/// The dangerous twin: the same eight shapes written wrong. Without this, a
/// harness that silently detects nothing would pass the test above.
#[test]
fn the_defective_twin_fires_every_rule_exactly_once() {
    assert_eq!(
        fired_distinct(DEFECTIVE_TWIN, Dialect::CommonLisp),
        RULE_NAMES.to_vec()
    );
}

/// Every finding this batch produces on the repository's own committed `.lisp`
/// fixtures, each one validated against CLHS by hand.
///
/// One entry, and it is a true positive:
///
/// - `tests/fixtures/corpus/deep-nesting.lisp:47` writes
///   `(with-open-file (stream path :direction :input :if-does-not-exist nil) …)`.
///   CLHS gives `open`'s `:direction` a default of `:input`, so the pair
///   restates it. The neighbouring `:if-does-not-exist nil` is written
///   explicitly, so it does not change if the `:direction` goes; the form is
///   equivalent without it.
///
/// This list is the gate. A new entry appearing means either a new fixture or a
/// new false positive, and either way somebody has to look.
const REVIEWED_FIXTURE_FINDINGS: [(&str, &str); 1] = [(
    "deep-nesting.lisp",
    "with-open-file-redundant-direction-default",
)];

/// The repository's own committed `.lisp` fixtures, which are ordinary correct
/// Common Lisp nobody wrote with these rules in mind.
///
/// Read from the workspace root rather than embedded, so the sweep tracks the
/// fixtures as they change. A checkout without them (a published crate, say)
/// skips the file half; the two embedded corpora above are the load-bearing
/// part and always run.
#[test]
fn the_repositorys_own_lisp_fixtures_produce_only_reviewed_findings() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut scanned = 0_usize;
    let mut seen: Vec<(String, &'static str)> = Vec::new();
    for directory in [
        workspace.join("tests/fixtures/corpus"),
        workspace.join("tests/fixtures/semantic_coverage_corpus"),
        workspace.join("fuzz/corpus/parse"),
        workspace.join("fuzz/corpus/format_idempotence"),
    ] {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "lisp") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            // A fixture deliberately containing unbalanced or exotic reader
            // syntax is not this sweep's subject.
            if SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).is_err() {
                continue;
            }
            scanned += 1;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            for rule in fired(&source, Dialect::CommonLisp) {
                seen.push((name.clone(), rule));
            }
        }
    }
    seen.sort_unstable();
    let mut expected: Vec<(String, &'static str)> = REVIEWED_FIXTURE_FINDINGS
        .iter()
        .map(|(file, rule)| ((*file).to_owned(), *rule))
        .collect();
    expected.sort_unstable();
    assert_eq!(seen, expected);

    // Not an assertion that files exist — see the doc comment — but a loud
    // note in the common case where they do.
    assert!(
        scanned > 0 || !workspace.join("tests/fixtures").exists(),
        "the fixture directories exist but nothing was scanned"
    );
}

// ---------------------------------------------------------------------------
// Cost
// ---------------------------------------------------------------------------

/// The eight new rules plus three already-shipped ones as controls, so a
/// measurement has something to be read against rather than an absolute number
/// nobody can calibrate.
///
/// `make-array-default-keyword` reads only its matched node's keyword slots —
/// the cheapest shape in the package. `empty-let` reads one child.
/// `the-arity` counts children. All three are single-head rules.
static MEASURED_ENTRIES: [RuleEntry; 11] = [
    RuleEntry::new(
        &crate::destructuring_bind_unused_whole::rule::META,
        &crate::destructuring_bind_unused_whole::rule::RULE,
    ),
    RuleEntry::new(
        &crate::flet_single_use_inlinable::rule::META,
        &crate::flet_single_use_inlinable::rule::RULE,
    ),
    RuleEntry::new(
        &crate::ftype_values_arity_mismatch::rule::META,
        &crate::ftype_values_arity_mismatch::rule::RULE,
    ),
    RuleEntry::new(
        &crate::loop_collect_into_immediately_returned::rule::META,
        &crate::loop_collect_into_immediately_returned::rule::RULE,
    ),
    RuleEntry::new(
        &crate::multiple_value_setq_arity_mismatch::rule::META,
        &crate::multiple_value_setq_arity_mismatch::rule::RULE,
    ),
    RuleEntry::new(
        &crate::quoted_form_contains_stray_unquote::rule::META,
        &crate::quoted_form_contains_stray_unquote::rule::RULE,
    ),
    RuleEntry::new(
        &crate::with_accessors_empty_binding_list::rule::META,
        &crate::with_accessors_empty_binding_list::rule::RULE,
    ),
    RuleEntry::new(
        &crate::with_open_file_redundant_direction_default::rule::META,
        &crate::with_open_file_redundant_direction_default::rule::RULE,
    ),
    // Controls.
    RuleEntry::new(
        &crate::make_array_default_keyword::rule::META,
        &crate::make_array_default_keyword::rule::RULE,
    ),
    RuleEntry::new(&crate::empty_let::rule::META, &crate::empty_let::rule::RULE),
    RuleEntry::new(&crate::the_arity::rule::META, &crate::the_arity::rule::RULE),
];

/// `repeats` copies of a zero-finding block containing every head this batch
/// anchors on, plus every head the controls anchor on.
///
/// Zero findings on purpose: `clean/forms/*` is what the CI bench gate
/// measures, and it is exactly the per-file cost a rule pays when it matches
/// nothing.
fn clean_corpus(repeats: usize) -> String {
    clean_corpus_with(repeats, true)
}

/// `worst_case` decides what the `declaim`s look like.
///
/// With it set, every `declaim` carries a fixed-arity `(values …)` `ftype`, so
/// `ftype-values-arity-mismatch` pays its full path — the binary search over
/// `root_children` plus materializing the neighbouring form — on every single
/// one. That is the rule's worst case and not what ordinary code looks like.
///
/// With it clear, the `declaim`s are `(optimize …)`, which is what a real file
/// mostly contains, and the rule stops after three comparisons without touching
/// the tree at all. This is the shape the CI `clean/forms/*` gate actually
/// lints, and the difference between the two columns is the whole argument that
/// the rule's cost is not on that gate's path.
fn clean_corpus_with(repeats: usize, worst_case: bool) -> String {
    // One flat top-level form per head, so the balance is readable by eye.
    // Every one of them is correct code that fires nothing.
    const BLOCK: &str = concat!(
        "(defun whole-{i} (form) (destructuring-bind (&whole w tag) form (list w tag)))\n",
        "(defun slots-{i} (thing) (with-slots (name size) thing (list name size)))\n",
        "(defun accessors-{i} (thing) (with-accessors ((n name-of)) thing n))\n",
        "(defun stream-{i} (path) (with-open-file (s path :direction :output) (write-line \"x\" s)))\n",
        "(defun opener-{i} (path) (open path :direction :probe))\n",
        "(defun local-{i} (a b) (flet ((scrub (v) (string v))) (cons (scrub a) (scrub b))))\n",
        "(defun named-{i} (a) (labels ((step-down (v) (if (zerop v) 0 (step-down (1- v))))) (step-down a)))\n",
        "(defun setq-{i} (n) (multiple-value-setq (q r) (values (floor n 60) (mod n 60))))\n",
        "(defun gather-{i} (items) (loop for x in items collect x into acc when (plusp (length acc)) do (print acc) finally (return acc)))\n",
        "(defun plain-{i} (items) (loop for x in items collect x))\n",
        "{declaim}",
        "(defun tag-{i} (x) (values x))\n",
        "(defmacro build-{i} (name value) `(progn (defparameter ,name ',value) (push '(tag ,value) *tags*)))\n",
        // The three controls' heads, likewise clean.
        "(defun array-{i} () (make-array 4 :adjustable t))\n",
        "(defun bound-{i} (v) (let ((seen v)) seen))\n",
        "(defun typed-{i} (v) (the integer v))\n",
    );
    let declaim = if worst_case {
        "(declaim (ftype (function (integer) (values integer)) tag-{i}))\n"
    } else {
        "(declaim (optimize (speed 3) (safety 1)))\n"
    };
    let block = BLOCK.replace("{declaim}", declaim);
    let mut source = String::from("(in-package :bench)\n");
    for index in 0..repeats {
        source.push_str(&block.replace("{i}", &index.to_string()));
    }
    source
}

/// Per-rule wall time and invocation count over `source`.
fn measure(source: &str) -> Vec<(&'static str, u128, u64)> {
    use paredit_core_lint_engine::engine::{PassOptions, collect_lint_pass};

    let catalog = RuleCatalog::new(&MEASURED_ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("bench.lisp"),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("lint pass");
    let names: Vec<&'static str> = outcome
        .outcomes
        .into_iter()
        .map(|found| found.into_parts().0.rule)
        .collect();
    assert!(
        names.is_empty(),
        "the corpus must produce no findings, got {names:?}"
    );
    let timings = outcome.timings.expect("measure was requested");
    timings
        .entries()
        .map(|(position, elapsed, invocations)| {
            (
                MEASURED_ENTRIES[position].meta().name().as_str(),
                elapsed.as_micros(),
                invocations,
            )
        })
        .collect()
}

/// Prints the per-rule cost table and asserts the doubling ratio is linear.
///
/// `#[ignore]` because a wall-clock assertion in CI is a flake generator under
/// parallel load; this is a probe to run by hand:
///
/// ```text
/// cargo test -p paredit-feature-lint-form-shape --lib \
///   engine_pass_tests::rule_cost_is_linear_in_the_number_of_heads \
///   --release -- --ignored --nocapture
/// ```
#[test]
#[ignore = "wall-clock probe; run by hand with --ignored --nocapture"]
fn rule_cost_is_linear_in_the_number_of_heads() {
    // Warm the allocator and the parse path so the first table is not charged
    // for them.
    let _ = measure(&clean_corpus(50));

    let first = measure(&clean_corpus(500));
    let second = measure(&clean_corpus(1000));

    // The shape the CI `clean/forms/*` gate actually lints: same heads, but the
    // declaims are `(optimize …)` rather than checkable ftypes.
    let realistic = measure(&clean_corpus_with(500, false));

    println!(
        "\n{:<46} {:>9} {:>9} {:>7} {:>7} {:>11}",
        "rule", "us@500", "us@1000", "calls", "ratio", "us@500-real"
    );
    for (((name, small_us, small_calls), (_, large_us, _)), (_, real_us, _)) in
        first.iter().zip(second.iter()).zip(realistic.iter())
    {
        let ratio = if *small_us == 0 {
            f64::NAN
        } else {
            *large_us as f64 / *small_us as f64
        };
        println!(
            "{name:<46} {small_us:>9} {large_us:>9} {small_calls:>7} {ratio:>7.2} {real_us:>11}"
        );
    }
    let small_total: u128 = first.iter().map(|(_, elapsed, _)| elapsed).sum();
    let large_total: u128 = second.iter().map(|(_, elapsed, _)| elapsed).sum();
    let ratio = large_total as f64 / small_total as f64;
    println!(
        "{:<46} {small_total:>9} {large_total:>9} {:>7} {ratio:>7.2}\n",
        "TOTAL", ""
    );

    assert!(
        ratio < 3.0,
        "doubling the heads multiplied total rule time by {ratio:.2}; linear is ~2.0"
    );
}
