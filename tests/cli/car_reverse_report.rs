use super::*;

#[test]
fn cli_flags_car_reverse() {
    let dir = fresh_temp_dir("car-reverse-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"accessor_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // Both sub-spans were in the old JSON and stay in it; `accessor_span`
        // is the only place the car/first spelling is reported at all.
        .stdout(predicate::str::contains("\"accessor_span\""))
        .stdout(predicate::str::contains("\"list_span\""));
}

#[test]
fn cli_does_not_flag_nreverse() {
    let dir = fresh_temp_dir("car-reverse-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nreverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one car, not of a reverse" from
        // "no car at all".
        .stdout(predicate::str::contains("\"accessor_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("car-reverse-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-reverse")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("car-reverse-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_last() {
    let dir = fresh_temp_dir("car-reverse-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("car-reverse")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(car (last xs))\n");
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_car_reverse() {
    let dir = fresh_temp_dir("car-reverse-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn last [xs] (. xs (length xs)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "car-reverse", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_car_reverse_emits_sarif() {
    let dir = fresh_temp_dir("car-reverse-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse items))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "car-reverse", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/car-reverse/car-reverse\"",
        ))
        .stdout(predicate::str::contains(
            "car of a reverse copies the whole list to read one element; use (car (last x))",
        ));
}
