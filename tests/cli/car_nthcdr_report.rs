use super::*;

#[test]
fn cli_flags_car_nthcdr() {
    let dir = fresh_temp_dir("car-nthcdr-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr n items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-nthcdr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"car_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // Both operand spans were in the old JSON and stay in it.
        .stdout(predicate::str::contains("\"count_span\""))
        .stdout(predicate::str::contains("\"list_span\""));
}

#[test]
fn cli_does_not_flag_cdr_outer() {
    let dir = fresh_temp_dir("car-nthcdr-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cdr (nthcdr n x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-nthcdr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // A `cdr` outer accessor is not a `car`, so nothing was even counted;
        // the denominator says so rather than leaving it to be inferred.
        .stdout(predicate::str::contains("\"car_form_count\": 0"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("car-nthcdr-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr n x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-nthcdr")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("car-nthcdr-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_nth() {
    let dir = fresh_temp_dir("car-nthcdr-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr (+ i 1) xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("car-nthcdr")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(nth (+ i 1) xs)\n");
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_car_nthcdr() {
    let dir = fresh_temp_dir("car-nthcdr-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [n xs] (. xs n))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "car-nthcdr", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_car_nthcdr_emits_sarif() {
    let dir = fresh_temp_dir("car-nthcdr-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr n items))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "car-nthcdr", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/car-nthcdr/car-nthcdr\"",
        ))
        .stdout(predicate::str::contains(
            "car of an nthcdr is nth; (car (nthcdr n x)) is (nth n x)",
        ));
}
