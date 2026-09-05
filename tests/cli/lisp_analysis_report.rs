//! The thirteen reports about the parts of Common Lisp that are not forms.
//!
//! One module rather than thirteen: they share a fixture and a contract. Each
//! answers for Common Lisp alone, so the property worth pinning across all of
//! them is that a dialect they do not model is *labelled* rather than reported
//! as clean.

use super::*;
use std::path::Path;

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

/// `read-time-eval` is the one report in this module that models more than
/// Common Lisp: a Clojure `#=` dispatch is risk-classified the same way a
/// Common Lisp `#.` is, and a `.dir-locals.el` `eval:` key is risk-classified
/// too, even though neither is a Common Lisp construct.
#[test]
fn cli_read_time_eval_models_a_clojure_read_eval_dispatch_too() {
    let dir = fresh_temp_dir("inspect-read-time-eval-clojure");
    let file = dir.join("core.clj");
    fs::write(&file, "(def x #=(System/currentTimeMillis))\n").expect("write clojure fixture");

    paredit()
        .args(["inspect", "read-time-eval", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": true"))
        .stdout(predicate::str::contains("\"risk\": \"live\""))
        .stdout(predicate::str::contains(
            "\"head\": \"System/currentTimeMillis\"",
        ));
}

/// A `.dir-locals.el` `eval:` key is risk-classified the same way as a
/// reader-dispatch read-time-eval, even though detecting it is a known-key
/// lookup rather than a reader dispatch.
#[test]
fn cli_read_time_eval_models_a_dir_locals_eval_key_too() {
    let dir = fresh_temp_dir("inspect-read-time-eval-dir-locals");
    let file = dir.join(".dir-locals.el");
    fs::write(&file, "((nil . ((eval . (delete-file \"x\")))))\n")
        .expect("write dir-locals fixture");

    paredit()
        .args(["inspect", "read-time-eval", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": true"))
        .stdout(predicate::str::contains("\"risk\": \"live\""))
        .stdout(predicate::str::contains("\"head\": \"delete-file\""));
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

// --- FR-005/FR-006: the macro-hygiene risks as enforceable lint rules.

/// Common Lisp's four macro-expander heads, each carrying the same capture.
/// `define-compiler-macro`, `define-setf-expander` and `defsetf` are
/// macro-expander definitions to `is_macro_expander_definition` but were
/// missing from the rule's head pre-filter, so the rule used to see one of the
/// four forms the report sees.
const EXPANDER_HEADS_COMMON_LISP: &str = "\
(defmacro plain-macro (form)
  `(let ((result ,form))
     (list result result)))

(define-compiler-macro cm-macro (form)
  `(let ((result ,form))
     (list result result)))

(define-setf-expander se-macro (form)
  `(let ((result ,form))
     (list result result)))

(defsetf setf-macro (form)
  `(let ((result ,form))
     (list result result)))
";

/// Emacs Lisp's three heads beyond the `defmacro` it shares with Common Lisp.
/// The `declare` form keeps `elisp-macro-missing-declare` out of the way, so
/// the only risk in the file is the capture this test counts.
const EXPANDER_HEADS_EMACS_LISP: &str = "\
(cl-defmacro cl-macro (form)
  (declare (indent 1))
  `(let ((result ,form))
     (list result result)))

(cl-define-compiler-macro cl-cm-macro (form)
  (declare (indent 1))
  `(let ((result ,form))
     (list result result)))

(define-inline inline-macro (form)
  (declare (indent 1))
  `(let ((result ,form))
     (list result result)))
";

/// LFE's `defsyntax`, which no other modelled dialect has.
const EXPANDER_HEADS_LFE: &str = "\
(defsyntax lfe-macro (form)
  `(let ((result ,form))
     (list result result)))
";

/// Fennel spells its macro-expander form `macro`, which means nothing in the
/// other seven — the head most likely to be dropped from the filter as noise.
const EXPANDER_HEADS_FENNEL: &str = "\
(macro fennel-macro [form]
  `(let [result ,form]
     [result result]))
";

/// One macro per risk the four new rules report.
const RISKS_FIXTURE: &str = "\
(defmacro twice (x)
  `(if (> ,x 0) ,x 0))

(defmacro reordered (a b)
  `(list ,b ,a))

(defmacro deep ()
  ```(a))
";

fn write_fixture(name: &str, file_name: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join(file_name);
    fs::write(&file, source).expect("write fixture");
    file
}

fn lint_json(args: &[&str], file: &Path) -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "lint", "--output", "json"])
        .args(args)
        .arg(file)
        .output()
        .expect("run inspect lint");
    serde_json::from_slice(&output.stdout).expect("inspect lint JSON is valid")
}

fn hygiene_report_risk_count(file: &Path, risk: &str) -> usize {
    let output = paredit()
        .args(["inspect", "macro-hygiene", "--output", "json"])
        .arg(file)
        .output()
        .expect("run inspect macro-hygiene");
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inspect macro-hygiene JSON is valid");
    value["files"]
        .as_array()
        .expect("files array")
        .iter()
        .flat_map(|file| file["findings"].as_array().expect("findings array"))
        .filter(|finding| finding["risk"] == risk)
        .count()
}

/// The rule's head pre-filter is a hard dispatch gate, so a head missing from
/// it costs findings silently. `inspect lint` must therefore see exactly what
/// `inspect macro-hygiene` sees — the two share one detection, and the only
/// thing that can make them disagree is the pre-filter.
/// Every head in the rule slice's `HEADS`, written in the dialect that owns
/// it. Nine heads across four dialects, with the capture count each file must
/// produce pinned as a literal: asserting only `reported > 0` beside
/// `linted == reported` passes when *both* sides regress together, which is
/// precisely the failure mode — a head dropped from the filter takes the same
/// finding out of the lint side and leaves the report's side to be compared
/// against.
#[test]
fn cli_the_capture_rule_sees_every_macro_expander_head_the_report_does() {
    for (label, file_name, source, expected) in [
        ("fr005-heads-cl", "m.lisp", EXPANDER_HEADS_COMMON_LISP, 4),
        ("fr005-heads-el", "m.el", EXPANDER_HEADS_EMACS_LISP, 3),
        ("fr005-heads-lfe", "m.lfe", EXPANDER_HEADS_LFE, 1),
        ("fr005-heads-fnl", "m.fnl", EXPANDER_HEADS_FENNEL, 1),
    ] {
        let file = write_fixture(label, file_name, source);
        let reported = hygiene_report_risk_count(&file, "variable-capture");
        assert_eq!(
            reported, expected,
            "{label}: the report must find one capture per macro-expander head"
        );
        let linted = lint_json(&["--rule", "macro-variable-capture"], &file);
        assert_eq!(linted["finding_count"], reported, "{label}: {linted}");
    }
}

/// Each new rule is selectable by name and reports its own risk and nothing
/// else. `--rule` is what makes the risk enforceable at all: it is the same
/// selector `lint.deny`, `lint.fail-on`, suppression and the baseline key on.
#[test]
fn cli_each_macro_hygiene_risk_is_its_own_lint_rule() {
    let file = write_fixture("fr006-risks", "m.lisp", RISKS_FIXTURE);

    for (rule, expected) in [
        ("macro-multiple-evaluation", "multiple evaluation: `X`"),
        ("macro-parameter-reordering", "parameter reordering:"),
        ("macro-deep-quasiquote-nesting", "deep quasiquote nesting:"),
    ] {
        let value = lint_json(&["--rule", rule], &file);
        assert_eq!(value["finding_count"], 1, "{rule}: {value}");
        assert_eq!(value["findings"][0]["rule"], rule, "{value}");
        let message = value["findings"][0]["message"]
            .as_str()
            .expect("a message string");
        assert!(message.contains(expected), "{rule}: {message}");

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
            .find(|listed| listed["rule"] == rule)
            .unwrap_or_else(|| panic!("{rule} is missing from --list-rules"));
        assert_eq!(entry["severity"], "warning", "{entry}");
        assert_eq!(entry["fixable"], false, "{entry}");
    }
}

/// The point of FR-006: a hygiene risk ships as a warning because it is a fact
/// about the file rather than a defect by definition, and a project that has
/// decided otherwise says so. `--deny` (what `lint.deny` sets) promotes it,
/// and `--fail-on` (what `lint.fail-on` sets) then changes the exit code.
#[test]
fn cli_a_macro_hygiene_rule_can_be_denied_and_failed_on() {
    let file = write_fixture("fr006-deny", "m.lisp", RISKS_FIXTURE);

    let plain = lint_json(&["--rule", "macro-multiple-evaluation"], &file);
    assert_eq!(plain["findings"][0]["severity"], "warning", "{plain}");

    let denied = lint_json(
        &[
            "--rule",
            "macro-multiple-evaluation",
            "--deny",
            "macro-multiple-evaluation",
        ],
        &file,
    );
    assert_eq!(denied["findings"][0]["severity"], "error", "{denied}");

    // A warning does not trip an error-level gate...
    //
    // `.code(0)`/`.code(3)` rather than `.success()`/`.failure()`: a usage
    // error, a parse refusal and a panic are all "failure", so `.failure()`
    // would pass on a run that never reached the gate at all.
    paredit()
        .args([
            "inspect",
            "lint",
            "--rule",
            "macro-multiple-evaluation",
            "--fail-on",
            "error",
        ])
        .arg(&file)
        .assert()
        .code(0);

    // ...but the same finding denied into an error does.
    paredit()
        .args([
            "inspect",
            "lint",
            "--rule",
            "macro-multiple-evaluation",
            "--deny",
            "macro-multiple-evaluation",
            "--fail-on",
            "error",
        ])
        .arg(&file)
        .assert()
        .code(3);
}

/// The same promotion driven from `paredit.toml` instead of the command line.
///
/// FR-006 names the *config keys* `lint.deny` and `lint.fail-on`, and the
/// flags above are a different code path into the same policy — a key that
/// failed to reach it would leave the flag test green. This is the shape a CI
/// pipeline actually uses: the file decides, and nobody types anything.
#[test]
fn cli_a_macro_hygiene_rule_can_be_denied_from_the_config_file() {
    let root = fresh_temp_dir("fr006-deny-config");
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
    fs::write(root.join("m.lisp"), RISKS_FIXTURE).expect("write fixture");

    let run = |root: &Path| {
        let mut command = paredit();
        command
            .current_dir(root)
            .args(["inspect", "lint", "--rule", "macro-multiple-evaluation"])
            .arg("m.lisp")
            .env_remove("PAREDIT_CONFIG_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env("HOME", root.display().to_string());
        command
    };

    // The control: the same fixture, no config, exits clean as a warning.
    run(&root).assert().code(0);

    fs::write(
        root.join("paredit.toml"),
        "[lint]\ndeny = [\"macro-multiple-evaluation\"]\nfail-on = \"error\"\n",
    )
    .expect("write config");

    run(&root).assert().code(3);
}

/// A suppression comment silences one of the new rules, which the standalone
/// report has no way to honour.
///
/// The control run comes first and asserts 1. Without it, "suppressed" and
/// "the rule stopped firing on this shape" are the same observation, and the
/// test would keep passing through a total regression of the rule.
#[test]
fn cli_a_suppression_comment_silences_a_macro_hygiene_rule() {
    const MACRO: &str = "(defmacro twice (x) `(if (> ,x 0) ,x 0))\n";

    let unsuppressed = write_fixture("fr006-suppress-control", "m.lisp", MACRO);
    let before = lint_json(&["--rule", "macro-multiple-evaluation"], &unsuppressed);
    assert_eq!(before["finding_count"], 1, "{before}");

    let suppressed = write_fixture(
        "fr006-suppress",
        "m.lisp",
        &format!(";; paredit:ignore macro-multiple-evaluation\n{MACRO}"),
    );
    let after = lint_json(&["--rule", "macro-multiple-evaluation"], &suppressed);
    assert_eq!(after["finding_count"], 0, "{after}");
}

/// `elisp-macro-missing-declare` carries the `elisp-` prefix because it is
/// scoped to Emacs Lisp alone: Common Lisp `defmacro` has no
/// `(declare (indent ...))`/`(declare (debug ...))` convention to omit.
#[test]
fn cli_the_missing_declare_rule_is_emacs_lisp_only() {
    let macro_source = "(defmacro no-declare (x) (list 'progn x))\n";

    let elisp = write_fixture("fr006-declare-el", "m.el", macro_source);
    let reported = lint_json(&["--rule", "elisp-macro-missing-declare"], &elisp);
    assert_eq!(reported["finding_count"], 1, "{reported}");
    assert_eq!(
        reported["findings"][0]["category"], "malformed",
        "{reported}"
    );

    let common_lisp = write_fixture("fr006-declare-cl", "m.lisp", macro_source);
    let silent = lint_json(&["--rule", "elisp-macro-missing-declare"], &common_lisp);
    assert_eq!(silent["finding_count"], 0, "{silent}");
}

/// `--explain` answers "why did this rule find nothing?" with the dialect
/// list, which is the whole difference between the four multi-dialect rules
/// and the Emacs-Lisp-only one.
#[test]
fn cli_explain_lists_the_dialects_each_macro_hygiene_rule_covers() {
    let explain = |rule: &str| -> serde_json::Value {
        let output = paredit()
            .args(["inspect", "lint", "--explain", rule, "--output", "json"])
            .output()
            .expect("run inspect lint --explain");
        serde_json::from_slice(&output.stdout).expect("--explain JSON is valid")
    };

    for rule in [
        "macro-multiple-evaluation",
        "macro-parameter-reordering",
        "macro-deep-quasiquote-nesting",
    ] {
        let value = explain(rule);
        let dialects: Vec<&str> = value["dialects"]
            .as_array()
            .expect("dialects array")
            .iter()
            .map(|dialect| dialect.as_str().expect("a dialect name"))
            .collect();
        assert!(dialects.contains(&"common-lisp"), "{rule}: {dialects:?}");
        assert!(dialects.contains(&"clojure"), "{rule}: {dialects:?}");
        // Scheme and Racket give hygiene as a language guarantee, so the
        // analysis deliberately does not model them.
        assert!(!dialects.contains(&"scheme"), "{rule}: {dialects:?}");
        assert!(!dialects.contains(&"racket"), "{rule}: {dialects:?}");
    }

    let declare = explain("elisp-macro-missing-declare");
    assert_eq!(declare["dialects"], serde_json::json!(["emacs-lisp"]));
}

/// A file carrying every risk the four multi-dialect rules report: a capture,
/// a doubly-unquoted parameter, a reordered pair, and a triple quasiquote.
const ALL_RISKS_FIXTURE: &str = "\
(defmacro twice (x)
  `(if (> ,x 0) ,x 0))

(defmacro reordered (a b)
  `(list ,b ,a))

(defmacro deep ()
  ```(a))

(defmacro capturing (form)
  `(let ((result ,form))
     (list result result)))
";

/// `--explain` reports the *declared* scope, which is a `RuleDialectScope`
/// value and not evidence that the dispatcher honours it. This runs the same
/// source through the engine: identical bytes, four extensions, and the two
/// hygienic dialects must report nothing at all.
///
/// Scheme and Racket are excluded because `syntax-rules` makes hygiene a
/// language guarantee — but they also both *parse* a `defmacro`-shaped form
/// perfectly well, so nothing but the scope stands between this fixture and
/// four findings per file.
#[test]
fn cli_no_macro_hygiene_rule_fires_on_scheme_or_racket() {
    const RULES: [&str; 5] = [
        "macro-variable-capture",
        "macro-multiple-evaluation",
        "macro-parameter-reordering",
        "macro-deep-quasiquote-nesting",
        "elisp-macro-missing-declare",
    ];

    // The control: the same bytes under a modelled dialect fire four of the
    // five, so a silent file below is the scope and not the fixture.
    let modelled = write_fixture("fr006-scope-control", "m.lisp", ALL_RISKS_FIXTURE);
    for rule in RULES {
        let value = lint_json(&["--rule", rule], &modelled);
        let expected = i64::from(rule != "elisp-macro-missing-declare");
        assert_eq!(value["finding_count"], expected, "{rule}: {value}");
    }

    for (label, file_name) in [("fr006-scope-scm", "m.scm"), ("fr006-scope-rkt", "m.rkt")] {
        let file = write_fixture(label, file_name, ALL_RISKS_FIXTURE);
        for rule in RULES {
            let value = lint_json(&["--rule", rule], &file);
            assert_eq!(value["finding_count"], 0, "{label}/{rule}: {value}");
        }
    }
}

/// `--fail-on-risk` is a gate over *every* risk, not over the two the flag's
/// help text used to name.
///
/// The fixture is chosen so it cannot pass by accident: an Emacs Lisp macro
/// with no quasiquoted template at all, whose only finding is
/// `missing-editor-declaration` — the mildest of the five, the one scoped to a
/// single dialect, and the one a reader of the old wording would have expected
/// the gate to ignore. It exits 3.
#[test]
fn cli_the_hygiene_gate_fires_on_a_missing_declaration_alone() {
    let file = write_fixture(
        "fr007-gate-breadth",
        "m.el",
        "(defmacro no-declare (x) (list 'progn x))\n",
    );

    // The premise: this file carries exactly one risk, and it is not capture
    // or multiple evaluation.
    let risks: Vec<String> = {
        let output = paredit()
            .args(["inspect", "macro-hygiene", "--output", "json"])
            .arg(&file)
            .output()
            .expect("run inspect macro-hygiene");
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("inspect macro-hygiene JSON is valid");
        value["files"]
            .as_array()
            .expect("files array")
            .iter()
            .flat_map(|entry| entry["findings"].as_array().expect("findings array"))
            .map(|finding| finding["risk"].as_str().expect("a risk label").to_owned())
            .collect()
    };
    assert_eq!(risks, ["missing-editor-declaration"]);

    // Ungated, the same run is clean...
    paredit()
        .args(["inspect", "macro-hygiene"])
        .arg(&file)
        .assert()
        .code(0);

    // ...and the gate is what turns it into a failure.
    paredit()
        .args(["inspect", "macro-hygiene", "--fail-on-risk"])
        .arg(&file)
        .assert()
        .code(3);
}
