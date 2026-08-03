use super::assert_format_output;
use super::{fresh_temp_dir, fs, paredit, predicate};
use std::path::Path;

/// The `--write` path is where a dropped reader prefix actually destroys
/// someone's file, so it is pinned through the real binary and against the
/// bytes on disk, not only through the library.
///
/// `treefmt` runs exactly this command over this repository's own tracked Lisp
/// files (`flake.nix`, `mkFormatFilesFor`), so a formatter that changes meaning
/// is a build hazard as well as a correctness defect.
#[test]
fn cli_format_write_keeps_a_reader_prefix_on_a_binding_list() {
    let dir = fresh_temp_dir("format-write-binding-prefix");
    let file = dir.join(Path::new("macro.lisp"));
    fs::write(&file, "(let `(a b) x)\n").expect("write fixture");

    paredit()
        .args(["edit", "format", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read rewritten fixture"),
        "(let `(a b)\n  x)\n"
    );
}

/// `(a ')` is a truncated form, and is refused rather than rewritten to `(a)`.
///
/// The refusal matters more than the diagnostic: the old behaviour left a
/// document that parsed, so `--write` replaced the file, `--check` called it
/// formatted, and nothing downstream had any way to notice the quote was gone.
#[test]
fn cli_format_refuses_a_reader_prefix_before_a_closing_delimiter() {
    let dir = fresh_temp_dir("format-dangling-prefix");
    let file = dir.join(Path::new("truncated.lisp"));
    fs::write(&file, "(a ')\n").expect("write fixture");

    paredit()
        .args(["edit", "format", "--write", "--file"])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is missing a form"));

    assert_eq!(
        fs::read_to_string(&file).expect("read untouched fixture"),
        "(a ')\n"
    );
}

#[test]
fn cli_formats_symbol_macrolet_indentation() {
    assert_format_output(
        "format-symbol-macrolet",
        "symbol-macrolet.lisp",
        "(symbol-macrolet ((value (compute value)) (used other)) (list value used))\n",
        "(symbol-macrolet ((value (compute value))\n                  (used other))\n  (list value used))\n",
    );
}

#[test]
fn cli_formats_macrolet_indentation() {
    assert_format_output(
        "format-macrolet",
        "macrolet.lisp",
        "(macrolet ((with-x (x) (list x outer))) (with-x 1) (with-x 2))\n",
        "(macrolet ((with-x (x)\n             (list x outer)))\n  (with-x 1)\n  (with-x 2))\n",
    );
}

#[test]
fn cli_formats_compiler_macrolet_indentation() {
    assert_format_output(
        "format-compiler-macrolet",
        "compiler-macrolet.lisp",
        "(compiler-macrolet ((with-x (x) (list x outer))) (with-x 1) (with-x 2))\n",
        "(compiler-macrolet ((with-x (x)\n                      (list x outer)))\n  (with-x 1)\n  (with-x 2))\n",
    );
}

#[test]
fn cli_formats_multiple_local_callable_bindings() {
    assert_format_output(
        "format-multiple-local-callables",
        "local-callables.lisp",
        "(macrolet ((with-a (x) (list x outer)) (with-b (y) (list y outer))) (with-a 1) (with-b 2))\n",
        "(macrolet ((with-a (x)\n             (list x outer))\n           (with-b (y)\n             (list y outer)))\n  (with-a 1)\n  (with-b 2))\n",
    );
}

#[test]
fn cli_formats_local_callable_bodies_on_dedicated_lines() {
    assert_format_output(
        "format-local-callable-bodies",
        "local-callable-bodies.lisp",
        "(labels ((parse (x) (validate x) (build x)) (emit (y) (write y) (finish))) (parse input) (emit output))\n",
        "(labels ((parse (x)\n           (validate x)\n           (build x))\n         (emit (y)\n           (write y)\n           (finish)))\n  (parse input)\n  (emit output))\n",
    );
}

#[test]
fn cli_formats_define_compiler_macro_indentation() {
    assert_format_output(
        "format-define-compiler-macro",
        "compiler-macro.lisp",
        "(define-compiler-macro fast-add (x y) (list '+ x y))\n",
        "(define-compiler-macro fast-add (x y)\n  (list '+ x y))\n",
    );
}

#[test]
fn cli_formats_define_setf_expander_indentation() {
    assert_format_output(
        "format-define-setf-expander",
        "setf-expander.lisp",
        "(define-setf-expander place (env) (values) (list place env))\n",
        "(define-setf-expander place (env)\n  (values)\n  (list place env))\n",
    );
}
