//! The five reports that print the semantic layer directly.
//!
//! One module rather than five: they share a fixture, and the property worth
//! pinning across all of them is the same — a dialect the layer does not model
//! must say so instead of returning an empty finding list that reads like a
//! clean bill of health.

use super::*;

/// A file exercising every layer: a folded constant, a declared type that
/// contradicts it, a narrowing test, and an effect that propagates one call up.
const FIXTURE: &str = "(defconstant +limit+ 10)\n\
     (defun scale (n) (* n (+ 1 2)))\n\
     (defun emit (n) (write-line (princ-to-string n)))\n\
     (defun run (n) (when (integerp n) (emit (scale n))))\n";

fn fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, FIXTURE).expect("write lisp fixture");
    file
}

#[test]
fn cli_types_pairs_a_declaration_against_the_propagated_value() {
    let dir = fresh_temp_dir("inspect-types");
    let file = dir.join("core.lisp");
    fs::write(
        &file,
        "(defun f () (let ((x 1)) (declare (type string x)) x))\n",
    )
    .expect("write lisp fixture");

    paredit()
        .args(["inspect", "types", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"declared\": \"string\""))
        .stdout(predicate::str::contains("\"inferred\": \"integer\""))
        .stdout(predicate::str::contains("\"contradictory\": true"));
}

#[test]
fn cli_types_fail_on_contradiction_trips_gate() {
    let dir = fresh_temp_dir("inspect-types-gate");
    let file = dir.join("core.lisp");
    fs::write(
        &file,
        "(defun f () (let ((x 1)) (declare (type string x)) x))\n",
    )
    .expect("write lisp fixture");

    paredit()
        .args(["inspect", "types", "--fail-on-contradiction"])
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("type-report policy failed"));
}

#[test]
fn cli_types_passes_the_gate_when_no_declaration_contradicts_inference() {
    paredit()
        .args([
            "inspect",
            "types",
            "--fail-on-contradiction",
            "--output",
            "json",
        ])
        .arg(fixture("inspect-types-clean"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"contradiction_count\": 0"));
}

#[test]
fn cli_narrowing_names_the_binding_the_branch_proves_something_about() {
    paredit()
        .args(["inspect", "narrowing", "--output", "json"])
        .arg(fixture("inspect-narrowing"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"site_count\": 1"))
        .stdout(predicate::str::contains("\"narrowed_to\": \"integer\""))
        .stdout(predicate::str::contains("\"branch\": \"then\""))
        .stdout(predicate::str::contains("\"source\": \"predicate\""));
}

#[test]
fn cli_narrowing_filters_to_one_binding() {
    paredit()
        .args([
            "inspect",
            "narrowing",
            "--binding",
            "nothing-by-this-name",
            "--output",
            "json",
        ])
        .arg(fixture("inspect-narrowing-filter"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"sites\": []"));
}

#[test]
fn cli_constants_reports_the_fold_and_the_file_level_constant() {
    paredit()
        .args(["inspect", "constants", "--output", "json"])
        .arg(fixture("inspect-constants"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"foldable_count\": 1"))
        .stdout(predicate::str::contains("\"value\": \"3\""))
        .stdout(predicate::str::contains("\"name\": \"+LIMIT+\""));
}

#[test]
fn cli_constants_min_saved_bytes_filters_small_folds() {
    paredit()
        .args([
            "inspect",
            "constants",
            "--min-saved-bytes",
            "1000",
            "--output",
            "json",
        ])
        .arg(fixture("inspect-constants-filter"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"foldable\": []"));
}

#[test]
fn cli_value_propagation_names_the_condition_each_binding_failed() {
    let dir = fresh_temp_dir("inspect-value-propagation");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f () (let ((x 1)) (setq x 2) x))\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "value-propagation", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"reason\": \"reassigned\""));
}

#[test]
fn cli_value_propagation_min_coverage_trips_gate() {
    let dir = fresh_temp_dir("inspect-value-propagation-gate");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f (a) a)\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "value-propagation", "--min-coverage", "0.9"])
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "value-propagation-report policy failed",
        ));
}

#[test]
fn cli_effects_propagates_an_effect_from_a_callee_to_its_caller() {
    paredit()
        .args(["inspect", "effects", "--output", "json"])
        .arg(fixture("inspect-effects"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"emit\""))
        .stdout(predicate::str::contains("\"cause\": \"write-line\""))
        .stdout(predicate::str::contains("\"inherited\": true"));
}

#[test]
fn cli_effects_filters_to_one_verdict() {
    paredit()
        .args(["inspect", "effects", "--purity", "pure", "--output", "text"])
        .arg(fixture("inspect-effects-filter"))
        .assert()
        .success()
        .stdout(predicate::str::contains("pure\t"))
        .stdout(predicate::str::contains("effectful\t/").not());
}

#[test]
fn cli_effects_fail_on_unknown_trips_gate() {
    let dir = fresh_temp_dir("inspect-effects-gate");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f (x) (mystery-macro x))\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "effects", "--fail-on-unknown"])
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("effect-report policy failed"));
}

/// The property that separates these reports from every other one: an empty
/// finding list is ambiguous, so a dialect outside the semantic layer's reach
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_every_semantic_report_labels_a_dialect_it_does_not_model() {
    let dir = fresh_temp_dir("inspect-semantic-unmodelled");
    let file = dir.join("core.clj");
    fs::write(&file, "(defn f [x] (when (string? x) x))\n").expect("write clojure fixture");

    for command in [
        "types",
        "narrowing",
        "constants",
        "value-propagation",
        "effects",
    ] {
        paredit()
            .args(["inspect", command, "--output", "json"])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains("\"dialect_modelled\": false"));
    }
}

/// Byte-identical output for byte-identical input. The semantic tables are
/// `HashMap`s, so every one of these reports imposes its own sort; without
/// that, two runs would differ and no baseline or diff could be trusted.
#[test]
fn cli_every_semantic_report_is_byte_identical_across_runs() {
    let file = fixture("inspect-semantic-determinism");

    for command in [
        "types",
        "narrowing",
        "constants",
        "value-propagation",
        "effects",
    ] {
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
