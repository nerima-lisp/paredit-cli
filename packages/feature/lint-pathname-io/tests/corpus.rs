//! The sweep that keeps this package honest on code it did not choose.
//!
//! Three checks, and the first two are a pair. A "no false positives" corpus
//! that happens to contain none of the operators these rules anchor on passes
//! trivially and proves nothing, so [`clean_corpus_reports_nothing`] also
//! asserts that *every rule was actually invoked* — the denominator comes from
//! the dispatcher's own per-rule invocation counter, not from a guess. The
//! dangerous twin then fires each rule exactly once over the same operator
//! vocabulary, so the pair distinguishes "found nothing because the code is
//! correct" from "found nothing because nothing was looked at".
//!
//! The third sweeps the repository's own Lisp fixtures. A finding there is not
//! automatically a bug in the rule; it is a claim about that fixture, which is
//! why [`FIXTURE_ALLOWLIST`] records reviewed ones rather than suppressing
//! them. A *new* finding fails this test and gets the same review.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use paredit_feature_lint_pathname_io as subject;

static ENTRIES: &[RuleEntry] = &[
    RuleEntry::new(
        &subject::pathname_built_by_concatenation::META,
        &subject::pathname_built_by_concatenation::RULE,
    ),
    RuleEntry::new(
        &subject::output_stream_without_if_exists::META,
        &subject::output_stream_without_if_exists::RULE,
    ),
    RuleEntry::new(
        &subject::pathname_component_compared_case_sensitively::META,
        &subject::pathname_component_compared_case_sensitively::RULE,
    ),
    RuleEntry::new(
        &subject::directory_without_wild_component::META,
        &subject::directory_without_wild_component::RULE,
    ),
    RuleEntry::new(
        &subject::with_open_file_result_captures_stream::META,
        &subject::with_open_file_result_captures_stream::RULE,
    ),
];

/// Idiomatic, correct Common Lisp that uses every operator these five rules
/// anchor on. Nothing here is a defect; every rule must stay silent, and every
/// rule must nonetheless be invoked.
const CLEAN_CORPUS: &str = r#"
(defpackage #:archive
  (:use #:common-lisp)
  (:export #:archive-directory #:load-index))

(in-package #:archive)

(defparameter *root* #p"/var/lib/archive/"
  "Where every archive file lives.")

(defun entry-path (name type)
  "Return the pathname of the archive entry called NAME with type TYPE."
  (merge-pathnames (make-pathname :name name :type type) *root*))

(defun index-path ()
  "Return the pathname of the archive index."
  (merge-pathnames (make-pathname :name "index" :type "sexp") *root*))

(defun source-file-p (path)
  "Return true when PATH names a Lisp source file."
  (and (string-equal (pathname-type path) "lisp")
       (not (string-equal (pathname-name path) "index"))))

(defun binary-file-p (path)
  "Return true when PATH names a compiled file."
  (equalp (pathname-type path) "fasl"))

(defun same-entry-name-p (left right)
  "Return true when LEFT and RIGHT are the same entry name."
  (string= (entry-name left) (entry-name right)))

(defun entry-status-p (entry status)
  "Return true when ENTRY carries STATUS."
  (equal (entry-status entry) status))

(defun entry-type-p (path wanted)
  "Return true when PATH has type WANTED, which the caller has already folded."
  (string= (pathname-type path) wanted))

(defun archive-directory (&optional (root *root*))
  "Return every file directly under ROOT."
  (directory (merge-pathnames (make-pathname :name :wild :type :wild) root)))

(defun archive-tree (&optional (root *root*))
  "Return every file under ROOT, recursively."
  (directory (merge-pathnames "**/*.*" root)))

(defun read-index (path)
  "Return every form in the index file at PATH."
  (with-open-file (stream path :direction :input :if-does-not-exist nil)
    (when stream
      (loop for form = (read stream nil :eof)
            until (eq form :eof)
            collect form))))

(defun write-index (path forms)
  "Write FORMS to the index file at PATH, replacing what was there."
  (ensure-directories-exist path)
  (with-open-file (stream path :direction :output
                               :if-exists :supersede
                               :if-does-not-exist :create)
    (dolist (form forms)
      (write form :stream stream)
      (terpri stream))
    (truename path)))

(defun append-entry (path form)
  "Add FORM to the end of the index file at PATH."
  (with-open-file (stream path :direction :output :if-exists :append
                               :if-does-not-exist :create)
    (write form :stream stream)
    (terpri stream)))

(defun copy-entry (from to)
  "Copy the archive entry FROM to TO, returning the number of bytes copied."
  (with-open-file (in from :element-type '(unsigned-byte 8))
    (with-open-file (out to :direction :output
                            :element-type '(unsigned-byte 8)
                            :if-exists :supersede)
      (let ((buffer (make-array 4096 :element-type '(unsigned-byte 8)))
            (total 0))
        (loop for read-count = (read-sequence buffer in)
              while (plusp read-count)
              do (write-sequence buffer out :end read-count)
                 (incf total read-count))
        total))))

(defun entry-lines (path)
  "Return every line of the archive entry at PATH, fully realized."
  (with-open-file (stream path :direction :input)
    (loop for line = (read-line stream nil)
          while line
          collect line)))

(defun parse-entry (text)
  "Return every form in TEXT."
  (with-input-from-string (stream text)
    (loop for form = (read stream nil :eof)
          until (eq form :eof)
          collect form)))

(defun render-entry (form)
  "Return FORM printed to a string."
  (with-output-to-string (stream)
    (write form :stream stream)))

(defun stream-summary (source)
  "Return a summary of the character stream SOURCE."
  (with-open-stream (stream source)
    (list :lines (loop for line = (read-line stream nil)
                       while line count line))))

(defun entry-age (path)
  "Return the write date of the archive entry at PATH, or NIL."
  (when (probe-file path)
    (file-write-date path)))

(defun retire-entry (name)
  "Move the archive entry called NAME aside, then remove the original."
  (let ((live (entry-path name "sexp"))
        (dead (entry-path name "sexp.old")))
    (when (probe-file live)
      (rename-file live dead)
      (when (probe-file dead)
        (delete-file dead)))))

(defun load-index (&optional (path (index-path)))
  "Load the index file at PATH if it is there."
  (when (probe-file path)
    (load path)))

(defun build-index (source)
  "Compile SOURCE and return the pathname of the compiled file."
  (compile-file source
                :output-file (merge-pathnames (make-pathname :type "fasl") source)))

(defun canonical-entry (designator)
  "Return DESIGNATOR as a pathname, resolved against the archive root."
  (let ((parsed (parse-namestring (string designator) nil *root*)))
    (merge-pathnames (pathname parsed) *root*)))

(defun open-entry-carefully (path)
  "Return an open stream on PATH, or NIL when it is not there."
  (open path :direction :input :if-does-not-exist nil))
"#;

/// The same operator vocabulary, written wrong. Each rule fires exactly once.
const DANGEROUS_TWIN: &str = r#"
(defpackage #:archive-broken (:use #:common-lisp))
(in-package #:archive-broken)

(defparameter *root* "/var/lib/archive")

(defun entry-path (name)
  "Glue the root and NAME together by hand."
  (open (concatenate 'string *root* "/" name)))

(defun write-index (path forms)
  "Write FORMS without saying what to do about an existing file."
  (with-open-file (stream path :direction :output)
    (dolist (form forms) (write form :stream stream))))

(defun source-file-p (path)
  "Compare the type case-sensitively."
  (string= (pathname-type path) "lisp"))

(defun archive-directory ()
  "Ask for a directory listing with no wild component."
  (directory "/var/lib/archive/"))

(defun entry-reader (path)
  "Hand back a closure over a stream that is already closed."
  (with-open-file (stream path)
    (lambda () (read-line stream nil))))
"#;

/// Findings in the repository's own fixtures that have been read and judged.
///
/// Entries are `(file name, rule name)`. Empty is the current, reviewed state:
/// none of the eight tracked `.lisp` fixtures does pathname or stream I/O at
/// all. A new entry means someone looked at a real finding and decided it was a
/// true positive worth keeping in the fixture — not that the rule was silenced.
const FIXTURE_ALLOWLIST: &[(&str, &str)] = &[];

fn catalog() -> RuleCatalog {
    RuleCatalog::new(ENTRIES)
}

/// One finding, as `(rule name, message)`.
type Finding = (String, String);

/// One rule's invocation count, as `(rule name, calls)`.
type Invocations = (&'static str, u64);

/// Every rule's findings and invocation count over one source.
fn run(source: &str, path: &Path) -> (Vec<Finding>, Vec<Invocations>) {
    let catalog = catalog();
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("corpus parses");

    let outcome = collect_lint_pass(
        catalog,
        &index,
        path,
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("corpus lints");

    let findings = outcome
        .outcomes
        .into_iter()
        .map(|outcome| {
            let (finding, _) = outcome.into_parts();
            (finding.rule.to_owned(), finding.message)
        })
        .collect();

    let invocations = outcome
        .timings
        .expect("measure: true yields timings")
        .entries()
        .map(|(position, _, calls)| (ENTRIES[position].meta().name().as_str(), calls))
        .collect();

    (findings, invocations)
}

/// The corpus is correct code, so nothing fires — *and* every rule was asked.
///
/// The second half is the part that cannot be faked. Without it a corpus
/// mentioning none of these operators would pass this test while proving
/// nothing at all about false positives.
#[test]
fn clean_corpus_reports_nothing() {
    let (findings, invocations) = run(CLEAN_CORPUS, Path::new("clean.lisp"));

    assert!(
        findings.is_empty(),
        "the clean corpus is idiomatic, correct Common Lisp; these are false positives:\n{findings:#?}"
    );

    for (rule, calls) in &invocations {
        assert!(
            *calls > 0,
            "{rule} was never invoked on the clean corpus, so its silence there says nothing; \
             add a form using one of its heads"
        );
    }

    // A floor against erosion, not a target. The corpus exercises the five
    // rules 48 times as written; 40 leaves room to reword a function without
    // failing this, while still catching someone gutting the corpus to make a
    // false positive go away.
    let total: u64 = invocations.iter().map(|(_, calls)| calls).sum();
    assert!(
        total >= 40,
        "the clean corpus exercised the rules only {total} times, down from the 48 it was \
         written to produce; it is being eroded and is no longer evidence of anything. \
         Per rule: {invocations:?}"
    );
}

/// The twin fires every rule, exactly once each.
#[test]
fn dangerous_twin_fires_each_rule_once() {
    let (findings, _) = run(DANGEROUS_TWIN, Path::new("twin.lisp"));

    let mut fired: Vec<&str> = findings.iter().map(|(rule, _)| rule.as_str()).collect();
    fired.sort_unstable();

    let mut expected: Vec<&str> = ENTRIES
        .iter()
        .map(|entry| entry.meta().name().as_str())
        .collect();
    expected.sort_unstable();

    assert_eq!(
        fired, expected,
        "each rule must fire exactly once on the dangerous twin; got:\n{findings:#?}"
    );
}

/// Quoted data is not code, and every rule's `check` must say so.
///
/// This runs through the real dispatcher rather than calling `examine`, which
/// is the whole point: the quote guard lives in `check`, so a per-rule unit
/// test of `examine` cannot reach it and a broken guard would be invisible.
/// Each line here is a defect this package reports when it is code.
#[test]
fn quoted_data_is_never_reported() {
    const QUOTED: &str = r#"
(defmacro define-broken-examples ()
  "Every form below is data, not a call."
  '(progn
     (open (concatenate 'string root "/" name))
     (with-open-file (stream path :direction :output) (write x :stream stream))
     (string= (pathname-type path) "lisp")
     (directory "/var/log/")
     (with-open-file (stream path) (lambda () (read-line stream nil)))))

(defparameter *documentation-examples*
  '((open (concatenate 'string root "/" name))
    (directory "/var/log/"))
  "Examples shown in the manual.")
"#;

    let (findings, invocations) = run(QUOTED, Path::new("quoted.lisp"));
    assert!(
        findings.is_empty(),
        "quoted data is not code; these are false positives:\n{findings:#?}"
    );

    // The guard is only meaningful if the rules were reached and declined.
    // Without this, deleting every rule would also pass.
    let total: u64 = invocations.iter().map(|(_, calls)| calls).sum();
    assert!(
        total > 0,
        "no rule was invoked on the quoted fixture, so its silence says nothing"
    );
}

/// A comma escapes a quasiquote back into code, so a defect there is real.
///
/// The companion to the test above, and the reason the quote model is two
/// counters rather than one depth: a single counter reports one of these two
/// fixtures wrongly whichever way it is written.
#[test]
fn a_quasiquoted_form_with_a_comma_is_still_code() {
    const UNQUOTED: &str = r#"
(defmacro with-log-directory ((var) &body body)
  "Bind VAR to the log listing."
  `(let ((,var ,(directory "/var/log/")))
     ,@body))
"#;

    let (findings, _) = run(UNQUOTED, Path::new("unquoted.lisp"));
    let rules: Vec<&str> = findings.iter().map(|(rule, _)| rule.as_str()).collect();
    assert_eq!(
        rules,
        vec!["directory-without-wild-component"],
        "a comma escapes the quasiquote, so this call is evaluated and must be reported"
    );
}

/// Every `.lisp` fixture the repository tracks.
fn fixture_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures")
        .canonicalize()
        .expect("tests/fixtures exists");

    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "lisp") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// A finding in the repository's own fixtures is reviewed, not suppressed.
#[test]
fn repository_fixtures_hold_no_unreviewed_finding() {
    let files = fixture_files();
    assert!(
        !files.is_empty(),
        "found no .lisp fixtures to sweep; the path this test walks has moved"
    );

    let allowed: BTreeSet<(&str, &str)> = FIXTURE_ALLOWLIST.iter().copied().collect();
    let mut unreviewed = Vec::new();

    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        // A fixture that does not parse is another test's problem.
        if SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).is_err() {
            continue;
        }
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let (findings, _) = run(&source, file);
        for (rule, message) in findings {
            if !allowed.contains(&(name.as_str(), rule.as_str())) {
                unreviewed.push(format!("{name}: {rule}: {message}"));
            }
        }
    }

    assert!(
        unreviewed.is_empty(),
        "new findings in the repository's own fixtures. Read each one and decide: a true \
         positive belongs in FIXTURE_ALLOWLIST, a false positive is a bug in the rule.\n{}",
        unreviewed.join("\n")
    );
}
