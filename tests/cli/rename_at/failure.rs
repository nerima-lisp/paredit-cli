#[test]
fn cli_rename_at_rejects_unsupported_dialects_without_writing() {
    let input = "(";
    // Scheme and Racket are deliberately absent: they now have verified
    // rename-binding semantics and a binding table of their own, so a
    // `rename-at` there is refused for the *parse* failure below rather than
    // for the dialect.
    for (dialect, extension) in [
        ("emacs-lisp", "el"),
        ("lfe", "lfe"),
        ("clojure", "clj"),
        ("hy", "hy"),
        ("carp", "carp"),
        ("janet", "janet"),
        ("fennel", "fnl"),
    ] {
        let dir = fresh_temp_dir(&format!("rename-at-{dialect}-rejected"));
        let file = dir.join(format!("input.{extension}"));
        fs::write(&file, input).expect("write malformed non-Common-Lisp rename-at fixture");

        let mut cmd = paredit();
        cmd.arg("refactor")
            .arg("rename-at")
            .arg("--file")
            .arg(&file)
            .arg("--at")
            .arg("0")
            .arg("--to")
            .arg("thunk")
            .arg("--dialect")
            .arg(dialect)
            .arg("--write")
            .assert()
            .failure()
            .stderr(
                predicate::str::contains(
                    "rename-at currently supports Common Lisp, Scheme, and Racket only",
                )
                    .and(predicate::str::contains("failed to parse input").not()),
            );

        assert_eq!(
            fs::read_to_string(&file).expect("read unchanged non-Common-Lisp fixture"),
            input,
            "dialect: {dialect}"
        );
    }
}

#[test]
fn cli_rename_at_rejects_reader_conditionals_without_writing() {
    for (case, dispatch) in [("include", "#+"), ("exclude", "#-")] {
        let input = format!("{dispatch}enabled (let ((value 1)) value)\n");
        let dir = fresh_temp_dir(&format!("rename-at-reader-conditional-{case}"));
        let file = dir.join("input.lisp");
        fs::write(&file, &input).expect("write reader conditional fixture");

        let mut cmd = paredit();
        cmd.arg("refactor")
            .arg("rename-at")
            .arg("--file")
            .arg(&file)
            .arg("--at")
            .arg(byte_offset(&input, "value 1").to_string())
            .arg("--to")
            .arg("count")
            .arg("--write")
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "reader conditional {dispatch}"
            )));

        assert_eq!(
            fs::read_to_string(file).expect("read unchanged reader conditional fixture"),
            input,
            "reader conditional: {dispatch}"
        );
    }
}

#[test]
fn cli_rename_at_rejects_reader_conditionals_outside_selected_scope_without_writing() {
    for (case, dispatch) in [("include", "#+"), ("exclude", "#-")] {
        let input = format!("(let ((value 1)) value)\n(progn ({dispatch}enabled (ignored)))\n");
        let dir = fresh_temp_dir(&format!(
            "rename-at-reader-conditional-outside-selected-scope-{case}"
        ));
        let file = dir.join("input.lisp");
        fs::write(&file, &input).expect("write reader conditional fixture");

        let mut cmd = paredit();
        cmd.arg("refactor")
            .arg("rename-at")
            .arg("--file")
            .arg(&file)
            .arg("--at")
            .arg(byte_offset(&input, "value 1").to_string())
            .arg("--to")
            .arg("count")
            .arg("--write")
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "reader conditional {dispatch}"
            )));

        assert_eq!(
            fs::read_to_string(file).expect("read unchanged reader conditional fixture"),
            input,
            "reader conditional outside selected scope: {dispatch}"
        );
    }
}

#[test]
fn cli_rename_at_reader_conditional_rejection_leaves_no_staged_files() {
    let input = "#+enabled (let ((value 1)) value)\n";
    let dir = fresh_temp_dir("rename-at-reader-conditional-no-staged-files");
    let file = dir.join("input.lisp");
    fs::write(&file, input).expect("write reader conditional fixture");

    let mut cmd = paredit();
    cmd.arg("refactor")
        .arg("rename-at")
        .arg("--file")
        .arg(&file)
        .arg("--at")
        .arg(byte_offset(input, "value 1").to_string())
        .arg("--to")
        .arg("count")
        .arg("--write")
        .assert()
        .failure()
        .stderr(predicate::str::contains("reader conditional #+"));

    assert_eq!(
        fs::read_to_string(&file).expect("read unchanged reader conditional fixture"),
        input
    );

    let leftovers = fs::read_dir(&dir)
        .expect("list reader conditional fixture directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with(".input.lisp.paredit-tmp-")
                || name.starts_with(".input.lisp.paredit-bak-")
        })
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "unexpected staged files after reader conditional rejection: {leftovers:?}"
    );
}

#[test]
fn cli_rename_at_rejects_non_binding_targets_without_writing() {
    let input = "(let ((value 1)) (+ value missing))\n";
    let dir = fresh_temp_dir("rename-at-invalid-target");
    let file = dir.join("input.lisp");
    fs::write(&file, input).expect("write invalid rename-at fixture");

    let mut cmd = paredit();
    cmd.arg("refactor")
        .arg("rename-at")
        .arg("--file")
        .arg(&file)
        .arg("--at")
        .arg(byte_offset(input, "missing").to_string())
        .arg("--to")
        .arg("other")
        .arg("--write")
        .assert()
        .failure();

    assert_eq!(
        fs::read_to_string(file).expect("read unchanged fixture"),
        input
    );
}

#[test]
fn cli_rename_at_rejects_package_syntax_without_writing() {
    for (case, symbol) in [
        ("external", "pkg:foo"),
        ("internal", "pkg::foo"),
        ("keyword", ":foo"),
        ("uninterned", "#:foo"),
    ] {
        let input = format!("(defun {symbol} () ({symbol}))\n");
        let dir = fresh_temp_dir(&format!("rename-at-package-syntax-{case}"));
        let file = dir.join("input.lisp");
        fs::write(&file, &input).expect("write package syntax fixture");

        let mut cmd = paredit();
        cmd.arg("refactor")
            .arg("rename-at")
            .arg("--file")
            .arg(&file)
            .arg("--at")
            .arg(byte_offset(&input, symbol).to_string())
            .arg("--to")
            .arg("bar")
            .arg("--write")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "does not support package-qualified, keyword, or uninterned symbols",
            ));

        assert_eq!(
            fs::read_to_string(file).expect("read unchanged fixture"),
            input,
            "symbol: {symbol}"
        );
    }
}

#[test]
fn cli_rename_at_rejects_package_qualified_references_without_writing() {
    let input = "(defun foo () 1) (other:foo)\n";
    let dir = fresh_temp_dir("rename-at-qualified-reference");
    let file = dir.join("input.lisp");
    fs::write(&file, input).expect("write package-qualified fixture");

    let mut cmd = paredit();
    cmd.arg("refactor")
        .arg("rename-at")
        .arg("--file")
        .arg(&file)
        .arg("--at")
        .arg(byte_offset(input, "foo ()").to_string())
        .arg("--to")
        .arg("bar")
        .arg("--write")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot safely rename a symbol referenced through a package qualifier",
        ));

    assert_eq!(
        fs::read_to_string(file).expect("read unchanged package-qualified fixture"),
        input
    );
}

#[test]
fn cli_rename_at_rejects_lexical_name_conflicts_without_writing() {
    let input = "(let ((value 1) (count 2)) (+ value count))\n";
    let dir = fresh_temp_dir("rename-at-name-conflict");
    let file = dir.join("input.lisp");
    fs::write(&file, input).expect("write conflicting lexical fixture");

    let mut cmd = paredit();
    cmd.arg("refactor")
        .arg("rename-at")
        .arg("--file")
        .arg(&file)
        .arg("--at")
        .arg(byte_offset(input, "value 1").to_string())
        .arg("--to")
        .arg("count")
        .arg("--write")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "target conflicts with an existing binding in the same scope",
        ));

    assert_eq!(
        fs::read_to_string(file).expect("read unchanged conflicting lexical fixture"),
        input
    );
}
use super::*;
