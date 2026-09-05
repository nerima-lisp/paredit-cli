//! L1: a command that prints a unified diff prints it through a `Painter`.
//!
//! Seven commands once built their diff with `unified_diff` and printed the
//! result directly, so `--color always` produced byte-identical output to
//! `--color never`. Nothing caught it, and the reason is worth stating: a test
//! that only asserts "no color when told not to" passes *trivially* for a
//! command that can never color at all. It is the wrong half of the contract
//! to check.
//!
//! So both directions are asserted here, and the `always` direction is the one
//! carrying the regression. `ColorMode::Always` deliberately skips the isatty
//! check, which is what makes this testable at all: the commands must color a
//! piped stdout, so no pty is needed to observe it. `--color` is also read
//! from raw argv *after* the configuration layers resolve, so it wins outright
//! over a `paredit.toml` or `PAREDIT_OUTPUT_COLOR` anywhere on the machine —
//! the reason this test needs no environment scrubbing to be hermetic,
//! including against an ambient `NO_COLOR`.
//!
//! Each case also asserts its `never` output is non-empty. A fixture that
//! stopped producing a diff would otherwise fail as a *color* problem, which
//! is a confusing way to learn that a rule or a selector changed shape.

use super::*;

const ESC: u8 = 0x1b;

/// One diff-emitting command and the fixture that makes its diff non-empty.
struct Case {
    label: &'static str,
    /// `(name, contents)` pairs written into the case's own temp directory.
    files: &'static [(&'static str, &'static str)],
    /// Arguments, without `--color`. Every argument ending in `.lisp` is
    /// resolved against that directory, so the table stays readable.
    args: &'static [&'static str],
}

const CLASS: &str = "(defclass point ()\n  ((x :initarg :x)\n   (y :initarg :y)))\n";
const METHODS: &str = "(defmethod render ((p point)) p)\n\n(defmethod render ((s shape)) s)\n";
const PLAIN_DEFUN: &str = "(defun alpha (x) x)\n";
const UNUSED_PARAM: &str = "(defun alpha (x y) x)\n";
const CONSTANT_FOLD: &str = "(defun alpha () (+ 1 2))\n";
const MISINDENTED: &str = "(defun alpha (x)\n            x)\n";
const PATCH_FROM: &str = "(defun alpha () 1)\n";
const PATCH_TO: &str = "(defun alpha () 2)\n";

/// The seven report commands, followed by controls known to emit color.
///
/// The controls are not decoration: they prove the harness itself — the flag,
/// the piped capture, the ESC scan — can observe color at all, so a row
/// failing in the first group is a real finding rather than a broken test.
const CASES: &[Case] = &[
    Case {
        label: "generate accessors --diff",
        files: &[("point.lisp", CLASS)],
        args: &[
            "generate",
            "accessors",
            "--file",
            "point.lisp",
            "--path",
            "0",
            "--diff",
        ],
    },
    Case {
        label: "generate defpackage --diff",
        files: &[("pkg.lisp", PLAIN_DEFUN)],
        args: &["generate", "defpackage", "--file", "pkg.lisp", "--diff"],
    },
    Case {
        label: "generate defgeneric --diff",
        files: &[("generic.lisp", METHODS)],
        args: &["generate", "defgeneric", "--file", "generic.lisp", "--diff"],
    },
    Case {
        label: "generate docstring --diff",
        files: &[("doc.lisp", PLAIN_DEFUN)],
        args: &[
            "generate",
            "docstring",
            "--file",
            "doc.lisp",
            "--path",
            "0",
            "--diff",
        ],
    },
    Case {
        label: "refactor fold-constants --diff",
        files: &[("fold.lisp", CONSTANT_FOLD)],
        args: &["refactor", "fold-constants", "fold.lisp", "--diff"],
    },
    Case {
        label: "refactor patch --diff",
        files: &[
            ("from.lisp", PATCH_FROM),
            ("to.lisp", PATCH_TO),
            ("apply.lisp", PATCH_FROM),
        ],
        args: &[
            "refactor",
            "patch",
            "--from",
            "from.lisp",
            "--to",
            "to.lisp",
            "--apply-to",
            "apply.lisp",
            "--all",
            "--diff",
        ],
    },
    Case {
        label: "refactor add-ignore-declaration --diff",
        files: &[("ignore.lisp", UNUSED_PARAM)],
        args: &[
            "refactor",
            "add-ignore-declaration",
            "ignore.lisp",
            "--diff",
        ],
    },
    Case {
        label: "edit format --diff (control)",
        files: &[("fmt.lisp", MISINDENTED)],
        args: &["edit", "format", "--file", "fmt.lisp", "--diff"],
    },
    Case {
        label: "edit wrap --diff (control)",
        files: &[("wrap.lisp", PLAIN_DEFUN)],
        args: &[
            "edit",
            "wrap",
            "--file",
            "wrap.lisp",
            "--path",
            "0",
            "--diff",
        ],
    },
];

/// Runs one case under `--color <mode>`, returning raw stdout bytes.
fn stdout_with_color(case: &Case, mode: &str) -> Vec<u8> {
    let dir = fresh_temp_dir("color-consistency");
    for (name, contents) in case.files {
        fs::write(dir.join(name), contents).expect("write fixture");
    }

    let mut args: Vec<String> = case
        .args
        .iter()
        .map(|arg| {
            if arg.ends_with(".lisp") {
                dir.join(arg).display().to_string()
            } else {
                (*arg).to_owned()
            }
        })
        .collect();
    args.push("--color".to_owned());
    args.push(mode.to_owned());

    let output = paredit().args(&args).output().expect("run paredit");

    assert!(
        output.status.success(),
        "{} exited {:?} under --color {mode}\nstderr: {}",
        case.label,
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn every_diff_emitting_command_colors_when_color_is_forced_on() {
    for case in CASES {
        let stdout = stdout_with_color(case, "always");
        assert!(
            stdout.contains(&ESC),
            "{} printed no ANSI escape under --color always; \
             its diff is very likely printed without colorize_diff/Painter",
            case.label
        );
    }
}

#[test]
fn every_diff_emitting_command_stays_plain_when_color_is_forced_off() {
    for case in CASES {
        let stdout = stdout_with_color(case, "never");
        assert!(
            !stdout.is_empty(),
            "{} produced no output at all, so this case proves nothing about \
             color; its fixture no longer yields a diff",
            case.label
        );
        assert!(
            !stdout.contains(&ESC),
            "{} printed an ANSI escape under --color never",
            case.label
        );
    }
}
