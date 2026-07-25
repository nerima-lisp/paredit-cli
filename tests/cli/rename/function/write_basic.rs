use super::*;

#[test]
fn cli_writes_function_rename_across_files() {
    assert_write_case(WriteCase {
        fixture_name: "rename-function-write",
        dialect: None,
        from: "old-name",
        to: "new-name",
        input_files: &[
            FixtureFile {
                path: "core.lisp",
                contents: "(defun old-name (x) x)\n",
            },
            FixtureFile {
                path: "caller.el",
                contents: "(defun caller () (old-name 1) #'old-name (function old-name) old-name)\n",
            },
        ],
        expected_files: &[
            FixtureFile {
                path: "core.lisp",
                contents: "(defun new-name (x) x)\n",
            },
            FixtureFile {
                path: "caller.el",
                contents: "(defun caller () (new-name 1) #'new-name (function new-name) old-name)\n",
            },
        ],
        expected_definition_count: 1,
        expected_call_count: 3,
    });
}

#[test]
fn cli_writes_emacs_lisp_function_rename_without_touching_value_references() {
    assert_write_case(WriteCase {
        fixture_name: "rename-function-emacs-lisp-write",
        dialect: Some("emacs-lisp"),
        from: "helper",
        to: "renamed",
        input_files: &[
            FixtureFile {
                path: "core.el",
                contents: "(defun helper (x) x)\n",
            },
            FixtureFile {
                path: "caller.el",
                contents: "(defun caller () (helper 1) #'helper (function helper) helper)\n",
            },
        ],
        expected_files: &[
            FixtureFile {
                path: "core.el",
                contents: "(defun renamed (x) x)\n",
            },
            FixtureFile {
                path: "caller.el",
                contents: "(defun caller () (renamed 1) #'renamed (function renamed) helper)\n",
            },
        ],
        expected_definition_count: 1,
        expected_call_count: 3,
    });
}

#[test]
fn cli_writes_function_rename_skipping_labels_local_calls() {
    assert_write_case(WriteCase {
        fixture_name: "rename-function-labels-shadowing",
        dialect: None,
        from: "helper",
        to: "renamed",
        input_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun helper (x) x)\n(defun main () (labels ((helper (x) (helper x))) (helper 1)))\n(defun caller () (helper 2))\n",
        }],
        expected_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun renamed (x) x)\n(defun main () (labels ((helper (x) (helper x))) (helper 1)))\n(defun caller () (renamed 2))\n",
        }],
        expected_definition_count: 1,
        expected_call_count: 1,
    });
}

#[test]
fn cli_writes_function_rename_inside_flet_binding_bodies_only() {
    assert_write_case(WriteCase {
        fixture_name: "rename-function-flet-shadowing",
        dialect: None,
        from: "helper",
        to: "renamed",
        input_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun helper (x) x)\n(defun main () (flet ((helper (x) (helper x))) (helper 1)))\n",
        }],
        expected_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun renamed (x) x)\n(defun main () (flet ((helper (x) (renamed x))) (helper 1)))\n",
        }],
        expected_definition_count: 1,
        expected_call_count: 1,
    });
}

#[test]
fn cli_writes_function_designators_but_skips_quoted_data() {
    assert_write_case(WriteCase {
        fixture_name: "rename-function-quoted-data",
        dialect: None,
        from: "helper",
        to: "renamed",
        input_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun helper (x) x)\n(list '(helper 1) #'helper (function helper) `(helper ,value) (helper 2))\n",
        }],
        expected_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun renamed (x) x)\n(list '(helper 1) #'renamed (function renamed) `(helper ,value) (renamed 2))\n",
        }],
        expected_definition_count: 1,
        expected_call_count: 3,
    });
}

#[test]
fn cli_writes_function_rename_preserving_unquote_prefixes_inside_quasiquote() {
    assert_write_case(WriteCase {
        fixture_name: "rename-function-quasiquote-unquote",
        dialect: None,
        from: "helper",
        to: "renamed",
        input_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun helper (x) x)\n(defmacro build () `(list ,(helper 1) ,@(helper 2) (helper 3)))\n",
        }],
        expected_files: &[FixtureFile {
            path: "core.lisp",
            contents: "(defun renamed (x) x)\n(defmacro build () `(list ,(renamed 1) ,@(renamed 2) (renamed 3)))\n",
        }],
        expected_definition_count: 1,
        expected_call_count: 3,
    });
}

/// The refactor writers share the staged-write anchor with `paredit edit`, and
/// that anchor could not resolve the parent of a bare relative file name. This
/// is the entry point an agent uses when it renames a symbol in a file it just
/// listed from the working directory.
#[test]
fn cli_writes_function_rename_for_a_bare_relative_file_name() {
    let dir = fresh_temp_dir("rename-function-write-bare-relative");
    fs::write(
        dir.join("core.lisp"),
        "(defun old-name (x) (+ x 1))\n(defun caller (y) (old-name y))\n",
    )
    .expect("write source fixture");

    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "rename-function",
            "--from",
            "old-name",
            "--to",
            "new-name",
            "--write",
            "core.lisp",
        ])
        .assert()
        .success();

    let rewritten = fs::read_to_string(dir.join("core.lisp")).expect("read rewritten source");
    assert_eq!(
        rewritten,
        "(defun new-name (x) (+ x 1))\n(defun caller (y) (new-name y))\n"
    );
}
