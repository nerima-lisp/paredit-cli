//! The `migrate` namespace end to end.
//!
//! The two properties worth a process boundary: a recipe's dialect scope has
//! to survive into the file walk, and a project's own recipe directory has to
//! be reachable. Both are the difference between a codemod and a hazard.

use super::*;

fn workspace(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = fresh_temp_dir(name);
    for (file, source) in files {
        let path = dir.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, source).expect("write fixture");
    }
    dir
}

#[test]
fn list_reports_the_shipped_recipes_with_their_scope_and_origin() {
    let output = paredit()
        .args(["migrate", "list"])
        .output()
        .expect("run migrate list");
    assert!(output.status.success());

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let recipes = report["recipes"].as_array().expect("recipes");
    let cl_lib = recipes
        .iter()
        .find(|recipe| recipe["name"] == "elisp-cl-lib")
        .expect("elisp-cl-lib ships");
    assert_eq!(cl_lib["origin"], "built-in");
    assert_eq!(cl_lib["dialects"], serde_json::json!(["emacs-lisp"]));
}

#[test]
fn explain_prints_the_steps_in_the_order_they_will_run() {
    let output = paredit()
        .args(["migrate", "explain", "nil-conditionals"])
        .output()
        .expect("run migrate explain");
    assert!(output.status.success());

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let steps = report["steps"].as_array().expect("steps");
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["query"], "(if (not ?test) ?then nil)");
    assert_eq!(steps[1]["query"], "(if ?test ?then nil)");
}

#[test]
fn an_unknown_recipe_names_the_ones_that_exist() {
    paredit()
        .args(["migrate", "explain", "no-such-recipe"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nil-conditionals"));
}

#[test]
fn run_applies_the_steps_in_order_so_the_negated_case_wins_its_form() {
    let source = "(if (not p) a nil)\n(if q b nil)\n";
    let dir = workspace("migrate-order", &[("a.lisp", source)]);
    paredit()
        .args(["migrate", "run", "nil-conditionals", "--write"])
        .arg(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        "(unless p a)\n(when q b)\n"
    );
}

/// `(incf x)` → `(cl-incf x)` is a modernization in Emacs Lisp and breakage in
/// Common Lisp, where `incf` *is* the correct spelling. The recipe's
/// `:dialects` is the only thing standing between the two, so it gets a test
/// at the process boundary rather than only in the unit tests.
#[test]
fn a_recipe_skips_the_dialects_it_is_not_correct_for_and_says_how_many() {
    let dir = workspace(
        "migrate-scope",
        &[("a.el", "(incf counter)\n"), ("b.lisp", "(incf counter)\n")],
    );
    let output = paredit()
        .args(["migrate", "run", "elisp-cl-lib", "--write"])
        .arg(&dir)
        .output()
        .expect("run migrate run");
    assert!(output.status.success());

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["summary"]["filesScanned"], 2);
    assert_eq!(report["summary"]["filesOutOfScope"], 1);
    assert_eq!(report["summary"]["filesTouched"], 1);

    assert_eq!(
        fs::read_to_string(dir.join("a.el")).expect("read back"),
        "(cl-incf counter)\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("b.lisp")).expect("read back"),
        "(incf counter)\n",
        "a Common Lisp file must be left alone by an Emacs Lisp recipe"
    );
}

#[test]
fn run_writes_nothing_without_write_and_check_gates() {
    let source = "(if q b nil)\n";
    let dir = workspace("migrate-dry", &[("a.lisp", source)]);

    paredit()
        .args(["migrate", "run", "nil-conditionals"])
        .arg(&dir)
        .assert()
        .success();
    paredit()
        .args(["migrate", "run", "nil-conditionals", "--check"])
        .arg(&dir)
        .assert()
        .code(3);
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        source
    );
}

#[test]
fn a_recipe_run_twice_changes_nothing_the_second_time() {
    let dir = workspace("migrate-idempotent", &[("a.lisp", "(if q b nil)\n")]);
    for _ in 0..2 {
        paredit()
            .args(["migrate", "run", "nil-conditionals", "--write"])
            .arg(&dir)
            .assert()
            .success();
    }
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        "(when q b)\n"
    );
    paredit()
        .args(["migrate", "run", "nil-conditionals", "--check"])
        .arg(&dir)
        .assert()
        .success();
}

#[test]
fn a_project_recipe_is_listed_and_runnable() {
    let dir = workspace(
        "migrate-project",
        &[
            ("src/a.lisp", "(old-name 1 2)\n"),
            (
                "recipes/local.lisp",
                "(defmigration local-rename\n  :description \"a project's own\"\n  \
                 :steps ((:query (old-name ?args...) :rewrite (new-name ?args...))))\n",
            ),
        ],
    );

    let output = paredit()
        .args(["migrate", "list", "--recipes"])
        .arg(dir.join("recipes"))
        .output()
        .expect("run migrate list");
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let local = report["recipes"]
        .as_array()
        .expect("recipes")
        .iter()
        .find(|recipe| recipe["name"] == "local-rename")
        .expect("the project's recipe is listed");
    assert_ne!(local["origin"], "built-in");

    paredit()
        .args(["migrate", "run", "local-rename", "--recipes"])
        .arg(dir.join("recipes"))
        .arg("--write")
        .arg(dir.join("src"))
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.join("src/a.lisp")).expect("read back"),
        "(new-name 1 2)\n"
    );
}

#[test]
fn a_malformed_project_recipe_fails_the_run_rather_than_contributing_nothing() {
    let dir = workspace(
        "migrate-malformed",
        &[(
            "recipes/broken.lisp",
            "(defmigration broken :steps ((:query (a) :rewrite (b))))\n",
        )],
    );
    paredit()
        .args(["migrate", "list", "--recipes"])
        .arg(dir.join("recipes"))
        .assert()
        .failure()
        .stderr(predicate::str::contains(":description"));
}
