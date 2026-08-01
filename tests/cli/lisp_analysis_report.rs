//! The thirteen reports about the parts of Common Lisp that are not forms.
//!
//! One module rather than thirteen: they share a fixture and a contract. Each
//! answers for Common Lisp alone, so the property worth pinning across all of
//! them is that a dialect they do not model is *labelled* rather than reported
//! as clean.

use super::*;

/// One file carrying a defect for every report: a shadowed slot, an orphaned
/// auxiliary method, a package-lock shadow, a capturing macro, a
/// multiply-evaluating macro, a format mismatch, a conflicting `loop`, a
/// reader conditional, a `#.`, a reader label, and a mixed-case symbol.
///
/// The mixed-case symbol sits *outside* the `#+sbcl` guard on purpose. A
/// dialect-aware parse consumes a whole reader conditional into one opaque
/// atom, so a symbol inside one is not a node any report can see — which is
/// the honest answer, and not the one this test is about.
const FIXTURE: &str = r#"(defpackage :app (:use :cl) (:shadow #:list))
(defclass animal () ((name :initarg :name)))
(defclass fish (animal) ((name :initform "?") (fins)))
(defgeneric speak (x))
(defmethod speak ((x fish)) (format t "~A ~A~%" (slot-value x 'name)))
(defmethod speak :before ((x bird)) (write-line "ahem"))
(defmacro twice (form) `(progn ,form ,form))
(defmacro capture (form) `(let ((result ,form)) result))
(defvar *built* #.(get-universal-time))
(defvar *cycle* '(#1=(a b) #1#))
#+sbcl (defun on-sbcl () 1)
(defun parseJSON () 1)
(defun run (xs) (twice (poll)) (loop for x in xs collect x sum x))
(defun recover () (restart-case (run nil) (retry () 1)))
"#;

fn fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, FIXTURE).expect("write lisp fixture");
    file
}

/// Every command this module covers, so the cross-cutting tests below cannot
/// drift out of step with the list.
const COMMANDS: [&str; 13] = [
    "macro-expansion",
    "macro-hygiene",
    "loop",
    "format-directives",
    "read-conditionals",
    "read-time-eval",
    "circular-literals",
    "readtable-case",
    "package-locks",
    "method-combination",
    "class-hierarchy",
    "generic-dispatch",
    "restarts",
];

#[test]
fn cli_class_hierarchy_names_the_slot_a_subclass_shadows() {
    paredit()
        .args(["inspect", "class-hierarchy", "--output", "json"])
        .arg(fixture("inspect-class-hierarchy"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"FISH\""))
        .stdout(predicate::str::contains("\"NAME@ANIMAL\""))
        .stdout(predicate::str::contains("\"shadowed_slots\""))
        .stdout(predicate::str::contains("shadowing_class_count"));
}

#[test]
fn cli_method_combination_reports_the_auxiliary_method_with_no_primary() {
    paredit()
        .args(["inspect", "method-combination", "--output", "json"])
        .arg(fixture("inspect-method-combination"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"orphaned-auxiliary\""))
        .stdout(predicate::str::contains("\"qualifier\": \"before\""));
}

#[test]
fn cli_method_combination_fail_on_orphaned_trips_gate() {
    paredit()
        .args(["inspect", "method-combination", "--fail-on-orphaned"])
        .arg(fixture("inspect-method-combination-gate"))
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect method-combination policy failed",
        ));
}

#[test]
fn cli_macro_hygiene_reports_capture_and_multiple_evaluation_separately() {
    paredit()
        .args(["inspect", "macro-hygiene", "--output", "json"])
        .arg(fixture("inspect-macro-hygiene"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"variable-capture\""))
        .stdout(predicate::str::contains(
            "\"kind\": \"multiple-evaluation\"",
        ))
        .stdout(predicate::str::contains("\"subject\": \"RESULT\""));
}

#[test]
fn cli_macro_expansion_substitutes_the_template_at_the_call_site() {
    paredit()
        .args(["inspect", "macro-expansion", "--output", "json"])
        .arg(fixture("inspect-macro-expansion"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"expansion\": \"(progn (poll) (poll))\"",
        ));
}

#[test]
fn cli_format_directives_reports_a_missing_argument() {
    paredit()
        .args(["inspect", "format-directives", "--output", "json"])
        .arg(fixture("inspect-format-directives"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"verdict\": \"too-few-arguments\"",
        ))
        .stdout(predicate::str::contains("\"consumed\": 2"));
}

#[test]
fn cli_loop_reports_two_accumulation_verbs_that_disagree() {
    paredit()
        .args(["inspect", "loop", "--output", "json"])
        .arg(fixture("inspect-loop"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"kind\": \"conflicting-accumulation\"",
        ))
        .stdout(predicate::str::contains("\"conflicting_accumulations\""));
}

#[test]
fn cli_read_conditionals_names_the_feature_and_the_guarded_code() {
    paredit()
        .args(["inspect", "read-conditionals", "--output", "json"])
        .arg(fixture("inspect-read-conditionals"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"SBCL\""))
        .stdout(predicate::str::contains("\"kind\": \"include\""));
}

#[test]
fn cli_read_time_eval_separates_a_live_call_from_inert_data() {
    paredit()
        .args(["inspect", "read-time-eval", "--output", "json"])
        .arg(fixture("inspect-read-time-eval"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"risk\": \"live\""))
        .stdout(predicate::str::contains("\"head\": \"get-universal-time\""));
}

#[test]
fn cli_circular_literals_pairs_a_definition_with_its_reference() {
    paredit()
        .args(["inspect", "circular-literals", "--output", "json"])
        .arg(fixture("inspect-circular-literals"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"role\": \"definition\""))
        .stdout(predicate::str::contains("\"role\": \"reference\""));
}

#[test]
fn cli_readtable_case_reports_a_mixed_case_symbol() {
    paredit()
        .args(["inspect", "readtable-case", "--output", "json"])
        .arg(fixture("inspect-readtable-case"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"parseJSON\""))
        .stdout(predicate::str::contains("\"upcased\": \"PARSEJSON\""));
}

#[test]
fn cli_package_locks_reports_an_explicit_shadow_without_calling_it_undefined() {
    paredit()
        .args(["inspect", "package-locks", "--output", "json"])
        .arg(fixture("inspect-package-locks"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"collision\": \"shadow\""))
        .stdout(predicate::str::contains("\"undefined_behavior_count\": 0"));
}

#[test]
fn cli_package_locks_reports_a_redefinition_as_undefined_behavior() {
    let dir = fresh_temp_dir("inspect-package-locks-redefine");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun list (&rest xs) xs)\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "package-locks", "--fail-on-undefined-behavior"])
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect package-locks policy failed",
        ));
}

#[test]
fn cli_generic_dispatch_pairs_a_defgeneric_with_its_methods() {
    paredit()
        .args(["inspect", "generic-dispatch", "--output", "json"])
        .arg(fixture("inspect-generic-dispatch"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"SPEAK\""))
        .stdout(predicate::str::contains("\"declared_arity\": 1"))
        .stdout(predicate::str::contains("\"method_count\": 2"));
}

#[test]
fn cli_restarts_reports_a_restart_nothing_invokes() {
    paredit()
        .args(["inspect", "restarts", "--output", "json"])
        .arg(fixture("inspect-restarts"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"role\": \"uninvoked\""))
        .stdout(predicate::str::contains("\"name\": \"RETRY\""));
}

/// An empty finding list is ambiguous, so a dialect outside a report's reach
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_every_lisp_analysis_report_labels_a_dialect_it_does_not_model() {
    // Scheme rather than Clojure: `inspect macro-hygiene` now models Clojure's
    // `defmacro` (it is unhygienic-by-default and template-based, just like
    // Common Lisp's), so a Clojure fixture would no longer be unmodelled by
    // every report in `COMMANDS`. Scheme's `define-syntax`/`syntax-rules` is
    // hygienic by language guarantee and stays out of that report too, and
    // every other report here is Common Lisp only, so Scheme is still
    // unmodelled by all thirteen.
    let dir = fresh_temp_dir("inspect-lisp-analysis-unmodelled");
    let file = dir.join("core.scm");
    fs::write(&file, "(define (f x) (let loop ((y x)) (loop y)))\n").expect("write scheme fixture");

    for command in COMMANDS {
        paredit()
            .args(["inspect", command, "--output", "json"])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains("\"dialect_modelled\": false"));
    }
}

/// Byte-identical output for byte-identical input. Several of these analyses
/// collect through a map before reporting, so the ordering is imposed rather
/// than inherited; without it no baseline or diff of this output is meaningful.
#[test]
fn cli_every_lisp_analysis_report_is_byte_identical_across_runs() {
    let file = fixture("inspect-lisp-analysis-determinism");

    for command in COMMANDS {
        let run = || {
            paredit()
                .args(["inspect", command, "--output", "json"])
                .arg(&file)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone()
        };
        assert_eq!(run(), run(), "{command} is not deterministic");
    }
}

/// Every report names itself in its JSON, so an agent aggregating several of
/// them can tell which produced a finding.
#[test]
fn cli_every_lisp_analysis_report_names_itself_in_its_output() {
    let file = fixture("inspect-lisp-analysis-self-naming");

    for command in COMMANDS {
        paredit()
            .args(["inspect", command, "--output", "json"])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "\"report\": \"inspect {command}\""
            )));
    }
}

/// `macro-variable-capture` reports a capture but never rewrites one. An
/// automatic gensym rewrite was implemented and reverted: it could corrupt a
/// working macro, so the rule is registered `ReportOnly` and `--fix` must
/// leave the file exactly as written.
#[test]
fn cli_variable_capture_is_reported_but_not_auto_fixed() {
    // Named to avoid the substring this test checks for: `fresh_temp_dir`
    // embeds its argument in the directory path, which then shows up in the
    // JSON under `path` and would make a naive `contains("variable-capture")`
    // check on the whole report pass for the wrong reason.
    let dir = fresh_temp_dir("macro-gensym-report-only");
    let file = dir.join("m.lisp");
    let source = "(defmacro m (form) `(let ((result ,form)) (list result)))\n";
    fs::write(&file, source).expect("write fixture");

    let reported = paredit()
        .args(["inspect", "lint", "--rule", "macro-variable-capture"])
        .arg(&file)
        .output()
        .expect("run inspect lint");
    let stdout = String::from_utf8_lossy(&reported.stdout);
    assert!(
        stdout.contains("variable capture"),
        "the capture must still be reported: {stdout}"
    );

    paredit()
        .args([
            "inspect",
            "lint",
            "--rule",
            "macro-variable-capture",
            "--fix",
        ])
        .arg(&file)
        .output()
        .expect("run inspect lint --fix");
    assert_eq!(
        fs::read_to_string(&file).expect("read back"),
        source,
        "a report-only rule must not rewrite the file"
    );

    let catalog = paredit()
        .args(["inspect", "lint", "--list-rules", "--output", "json"])
        .output()
        .expect("run inspect lint --list-rules");
    let rules: serde_json::Value =
        serde_json::from_slice(&catalog.stdout).expect("--list-rules JSON is valid");
    let entry = rules["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .find(|rule| rule["rule"] == "macro-variable-capture")
        .expect("macro-variable-capture present");
    assert_eq!(entry["fixable"], false);
}
