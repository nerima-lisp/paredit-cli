//! Every edit that has an inverse, applied and then undone.
//!
//! Section I item I1. The individual command tests answer "does this edit do
//! the right thing to this input"; this module answers a question none of them
//! can: **does the pair compose back to where it started**. That is a different
//! property, and it is the one that catches the failure mode a per-command test
//! is blind to — an edit that is correct in isolation and slightly lossy, so
//! that repeated use drifts.
//!
//! It found two such defects when it was written. `when → if` wrapped a
//! one-form body in `progn` and `if → when` kept the `progn`, so alternating
//! them added a nesting level per cycle; and `cond → if` turned the catch-all
//! clause that `if → cond` had just written into a nested `(if (quote t) ...)`,
//! with the same unbounded growth. Both are fixed, and both are pinned here by
//! the *third* round trip rather than the first — one cycle would have looked
//! fine in the `cond` case.
//!
//! Three shapes of property appear below, and the difference matters:
//!
//! - **Exact inverse.** `inverse(forward(x)) == x`, byte for byte.
//! - **Involution.** `f(f(x)) == x` for an edit that is its own inverse.
//! - **Idempotent under repetition.** The pair reaches a fixed point and stays
//!   there, checked over several cycles. A pair that merely returns something
//!   *equivalent* still fails this if it grows.

use super::*;

/// Runs one `edit` subcommand over stdin and returns its stdout.
fn edit(command: &str, path: &str, input: &str) -> String {
    let output = paredit()
        .args(["edit", command, "--path", path])
        .write_stdin(input.to_owned())
        .output()
        .expect("run edit");
    assert!(
        output.status.success(),
        "`edit {command} --path {path}` failed on {input:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs one `refactor` conversion over a real file, which is the path these
/// commands are actually used through.
fn convert(dir: &std::path::Path, command: &str, dialect: &str) -> String {
    let source = dir.join("core.lisp");
    let assertion = paredit()
        .args(["refactor", command, "--dialect", dialect])
        .arg("--file")
        .arg(&source)
        .args(["--path", "0", "--write"])
        .assert();
    let output = assertion.get_output();
    assert!(
        output.status.success(),
        "`refactor {command}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(&source).expect("read converted source")
}

// --- exact inverses among the structural edits ---

#[test]
fn wrap_then_splice_restores_the_input() {
    for (input, path) in [
        ("(f a b)\n", "0.1"),
        ("(f (g x) b)\n", "0.1"),
        ("(defun f (x) (+ x 1))\n", "0.3"),
    ] {
        let wrapped = edit("wrap", path, input);
        assert_ne!(wrapped, input, "wrap must change {input:?}");
        assert_eq!(
            edit("splice", path, &wrapped),
            input,
            "round-tripping {input:?}"
        );
    }
}

#[test]
fn slurp_forward_then_barf_forward_restores_the_input() {
    for (input, path) in [
        ("((f a) b)\n", "0.0"),
        ("((f a) (g b))\n", "0.0"),
        ("((f) b c)\n", "0.0"),
    ] {
        let slurped = edit("slurp-forward", path, input);
        assert_ne!(slurped, input, "slurp must change {input:?}");
        assert_eq!(
            edit("barf-forward", path, &slurped),
            input,
            "round-tripping {input:?}"
        );
    }
}

/// Slurping backward consumes the previous sibling, so the list itself moves
/// one position earlier — the inverse has to be aimed at where it landed, not
/// where it was. Naming both paths is the point: an inverse that happens to be
/// spelled the same as the forward edit is the exception, not the rule.
#[test]
fn slurp_backward_then_barf_backward_restores_the_input() {
    for (input, path, moved) in [
        ("(a (f b))\n", "0.1", "0.0"),
        ("((g a) (f b))\n", "0.1", "0.0"),
        ("(x a (f b))\n", "0.2", "0.1"),
    ] {
        let slurped = edit("slurp-backward", path, input);
        assert_ne!(slurped, input, "slurp must change {input:?}");
        assert_eq!(
            edit("barf-backward", moved, &slurped),
            input,
            "round-tripping {input:?}"
        );
    }
}

/// The inverse acts on the *moved* form, which is now one position later —
/// which is exactly the sort of off-by-one an exact-inverse property catches.
#[test]
fn transpose_forward_then_backward_restores_the_input() {
    for (input, path, moved) in [
        ("(f a b)\n", "0.1", "0.2"),
        ("(f (g x) b c)\n", "0.1", "0.2"),
        ("(f a b c)\n", "0.2", "0.3"),
    ] {
        let transposed = edit("transpose-forward", path, input);
        assert_ne!(transposed, input, "transpose must change {input:?}");
        assert_eq!(
            edit("transpose-backward", moved, &transposed),
            input,
            "round-tripping {input:?}"
        );
    }
}

#[test]
fn split_then_join_restores_the_input() {
    for (input, split_at) in [("(f a b c)\n", "0.2"), ("(f a b)\n", "0.2")] {
        let split = edit("split", split_at, input);
        assert_ne!(split, input, "split must change {input:?}");
        assert_eq!(edit("join", "0", &split), input, "round-tripping {input:?}");
    }
}

/// `convolute` exchanges two levels of nesting, so applying it twice at the
/// same path puts them back.
#[test]
fn convolute_is_its_own_inverse() {
    for (input, path) in [
        ("(f (g (h x)))\n", "0.1.1"),
        ("(let ((a 1)) (when p (run)))\n", "0.2.2"),
    ] {
        let once = edit("convolute", path, input);
        assert_ne!(once, input, "convolute must change {input:?}");
        assert_eq!(edit("convolute", path, &once), input, "on {input:?}");
    }
}

// --- exact inverses among the conditional and binding conversions ---

/// Repeats a conversion pair several times.
///
/// Three cycles, not one: the `cond` defect this module found looked like a
/// clean round trip after a single cycle and grew a level on the next.
fn assert_conversion_pair_is_stable(
    label: &str,
    source: &str,
    dialect: &str,
    forward: &str,
    inverse: &str,
) {
    let dir = fresh_temp_dir(label);
    fs::write(dir.join("core.lisp"), source).expect("write source");

    for cycle in 1..=3 {
        let converted = convert(&dir, forward, dialect);
        assert_ne!(
            converted, source,
            "{forward} must change {source:?} on cycle {cycle}"
        );
        let restored = convert(&dir, inverse, dialect);
        assert_eq!(
            restored, source,
            "{forward}/{inverse} must restore {source:?}; cycle {cycle} produced {restored:?}"
        );
    }
}

#[test]
fn let_and_let_star_are_exact_inverses() {
    for source in [
        "(let ((a 1)) a)\n",
        "(let ((a 1) (b 2)) (+ a b))\n",
        "(let () nil)\n",
    ] {
        assert_conversion_pair_is_stable(
            "round-trip-let",
            source,
            "common-lisp",
            "convert-let-to-let-star",
            "convert-let-star-to-let",
        );
    }
}

#[test]
fn flet_and_labels_are_exact_inverses() {
    for source in [
        "(flet ((f (x) x)) (f 1))\n",
        "(flet ((f (x) x) (g (y) y)) (f (g 1)))\n",
    ] {
        assert_conversion_pair_is_stable(
            "round-trip-flet",
            source,
            "common-lisp",
            "convert-flet-to-labels",
            "convert-labels-to-flet",
        );
    }
}

/// The first defect this module found: `when → if` used to wrap a one-form
/// body in `progn`, and `if → when` kept it, so each cycle added a level.
#[test]
fn if_and_when_are_exact_inverses_for_a_single_form_body() {
    for source in ["(if (p x) (g x))\n", "(if ready yes)\n"] {
        assert_conversion_pair_is_stable(
            "round-trip-when",
            source,
            "common-lisp",
            "convert-if-to-when",
            "convert-when-to-if",
        );
    }
}

#[test]
fn if_and_unless_are_exact_inverses_for_a_single_form_body() {
    assert_conversion_pair_is_stable(
        "round-trip-unless",
        "(if (not (p x)) nil (g x))\n",
        "common-lisp",
        "convert-if-to-unless",
        "convert-unless-to-if",
    );
}

/// The second defect: `if → cond` writes a catch-all clause, and `cond → if`
/// used to turn it back into a nested `(if (quote t) ...)`.
#[test]
fn if_and_cond_are_exact_inverses() {
    for source in ["(if (p x) (g x) (h x))\n", "(if ready yes)\n"] {
        assert_conversion_pair_is_stable(
            "round-trip-cond",
            source,
            "common-lisp",
            "convert-if-to-cond",
            "convert-cond-to-if",
        );
    }
}

// --- generated inputs ---

proptest! {
    #![proptest_config(cli_proptest_config(24))]

    /// Over generated argument lists rather than three hand-picked ones. A
    /// wrap/splice that is exact for `(f a b)` and lossy for a form with a
    /// string or a nested list in it would pass every example above.
    #[test]
    fn wrap_then_splice_restores_any_generated_call(
        arguments in prop::collection::vec(
            prop_oneof![
                "[a-z][a-z0-9-]{0,6}".prop_map(|name| name),
                Just("(g 1)".to_owned()),
                Just("\"text\"".to_owned()),
                Just("'sym".to_owned()),
            ],
            1..5,
        ),
        target in 0usize..4,
    ) {
        let index = target % arguments.len();
        let input = format!("(f {})\n", arguments.join(" "));
        // Child 0 is `f`, so argument `index` sits at path `0.{index + 1}`.
        let path = format!("0.{}", index + 1);

        let wrapped = edit("wrap", &path, &input);
        prop_assert_ne!(&wrapped, &input);
        prop_assert_eq!(edit("splice", &path, &wrapped), input);
    }

    /// Transposing forward then back over a generated list, at a generated
    /// position.
    #[test]
    fn transpose_round_trips_at_any_generated_position(
        length in 2usize..6,
        target in 0usize..5,
    ) {
        let arguments = (0..length)
            .map(|index| format!("a{index}"))
            .collect::<Vec<_>>();
        let input = format!("(f {})\n", arguments.join(" "));
        // Leave at least one sibling to the right to exchange with.
        let index = target % (length - 1);
        let path = format!("0.{}", index + 1);
        let moved = format!("0.{}", index + 2);

        let transposed = edit("transpose-forward", &path, &input);
        prop_assert_ne!(&transposed, &input);
        prop_assert_eq!(edit("transpose-backward", &moved, &transposed), input);
    }

    /// Slurp and barf over a generated tail.
    #[test]
    fn slurp_then_barf_restores_any_generated_tail(
        tail in prop::collection::vec("[a-z][a-z0-9-]{0,5}", 1..5),
    ) {
        let input = format!("((f a) {})\n", tail.join(" "));

        let slurped = edit("slurp-forward", "0.0", &input);
        prop_assert_ne!(&slurped, &input);
        prop_assert_eq!(edit("barf-forward", "0.0", &slurped), input);
    }
}
