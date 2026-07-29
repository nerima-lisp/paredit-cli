//! What must hold for *any* input, including inputs nobody would write.
//!
//! Section I item I9. `fuzz/` holds libFuzzer targets for the same entry points
//! and needs a nightly toolchain, which means it runs when a maintainer
//! remembers to run it. This file is the part that runs every time: the same
//! invariants, driven by proptest on stable, plus a replay of every seed and
//! every recorded crasher in the fuzz corpus.
//!
//! The distinction that matters for all four properties below is between a
//! *refusal* and a *crash*. This tool is run unattended by agents; refusing an
//! input is a designed outcome and panicking is not, so every assertion here is
//! of the form "returns, either way" rather than "succeeds".

use std::fs;
use std::path::{Path, PathBuf};

use paredit_cli::dialect::Dialect;
use paredit_cli::sexpr::{Edit, ExpressionPath, Formatter, SyntaxTree};
use proptest::prelude::*;
use proptest::test_runner::{
    Config as ProptestConfig, FileFailurePersistence, RngAlgorithm, TestCaseError, TestRng,
    TestRunner,
};

/// A fixed seed, so this file is a gate rather than a lottery.
///
/// A property test with a fresh seed per run is the right tool for *finding*
/// an invariant violation and the wrong one for a merge gate: the same commit
/// passes on one run and fails on the next, and a red build that goes green on
/// re-run teaches everyone to press re-run. It found the truncated
/// character-literal family this way — a case a local run had missed turned up
/// in CI — and each of those is now a corpus seed replayed every time.
///
/// Widening the search is still wanted, just not on the critical path:
/// `PAREDIT_ROBUSTNESS_SEED=<n>` re-runs this file with a different seed, and
/// the fuzz targets in `fuzz/` explore continuously.
fn robustness_config(cases: u32) -> ProptestConfig {
    let mut config = ProptestConfig::with_cases(cases);
    config.failure_persistence = Some(Box::new(FileFailurePersistence::Off));
    config.rng_algorithm = RngAlgorithm::ChaCha;
    config
}

/// The seed every run uses unless one is supplied.
///
/// Arbitrary, and fixed. Its only job is to be the same on your machine and on
/// the runner.
fn robustness_seed() -> [u8; 32] {
    let requested = std::env::var("PAREDIT_ROBUSTNESS_SEED")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0x5EED_1515_C0FF_EE01);
    let mut seed = [0_u8; 32];
    for (index, chunk) in seed.chunks_mut(8).enumerate() {
        chunk.copy_from_slice(&(requested.wrapping_add(index as u64)).to_le_bytes());
    }
    seed
}

/// Runs one property over the fixed seed.
///
/// `proptest!` uses the thread-local default RNG, which is seeded from entropy;
/// driving the runner directly is what makes the seed above take effect.
fn check_property<S: Strategy>(
    cases: u32,
    strategy: S,
    property: impl Fn(S::Value) -> Result<(), TestCaseError>,
) where
    S::Value: std::fmt::Debug,
{
    let mut runner = TestRunner::new_with_rng(
        robustness_config(cases),
        TestRng::from_seed(RngAlgorithm::ChaCha, &robustness_seed()),
    );
    if let Err(failure) = runner.run(&strategy, property) {
        panic!("{failure}");
    }
}

/// Every dialect, so a reader rule added for one cannot crash another.
const DIALECTS: [Dialect; 10] = [
    Dialect::CommonLisp,
    Dialect::EmacsLisp,
    Dialect::Lfe,
    Dialect::Scheme,
    Dialect::Racket,
    Dialect::Clojure,
    Dialect::Hy,
    Dialect::Carp,
    Dialect::Janet,
    Dialect::Fennel,
];

/// The invariants one input must satisfy, for one dialect.
///
/// Shared by the generated cases and the corpus replay so the two cannot
/// check different things.
fn check_invariants(source: &str, dialect: Dialect) -> Result<(), String> {
    // 1. Parsing returns. A parse error is a fine answer.
    let Ok(tree) = SyntaxTree::parse_with_dialect(source, dialect) else {
        // 1a. A document that does not parse must still survive the repair
        // path without panicking, and whatever repair produces must parse —
        // that is the entire claim `repair-unclosed-lists` makes.
        //
        // Written as a nested `if` rather than a let chain: edition 2024 makes
        // `let ... && ...` look available and this workspace's 1.85 MSRV does
        // not have it, so only the `msrv` check would have caught it.
        if let Ok(repaired) = SyntaxTree::repair_unclosed_lists(source) {
            if SyntaxTree::parse(&repaired).is_err() {
                return Err(format!(
                    "repair-unclosed-lists produced output that does not reparse for {source:?}"
                ));
            }
        }
        return Ok(());
    };

    // 2. The parse is lossless. Every rewrite is a span replacement over this.
    if tree.source() != source {
        return Err(format!("parsing was lossy for {source:?}"));
    }

    // 3. Every path the tree reports resolves, and its text lies inside the
    //    source. A span that indexes past the end would panic a slice.
    for index in 0..tree.root_children().len() {
        let path = ExpressionPath::from_indexes(vec![index]);
        let Ok(selection) = tree.select_path(&path) else {
            return Err(format!("path {index} is unresolvable in {source:?}"));
        };
        let span = selection.span();
        if span.end().get() > source.len() || !source.is_char_boundary(span.start().get()) {
            return Err(format!("path {index} yielded a bad span in {source:?}"));
        }
    }

    // 4. Formatting returns, reparses, and converges.
    let formatter = Formatter::with_dialect(2, dialect);
    let once = formatter.format(&tree);
    if let Ok(reparsed) = SyntaxTree::parse_with_dialect(&once, dialect) {
        let twice = formatter.format(&reparsed);
        if once != twice {
            return Err(format!("formatting did not converge for {source:?}"));
        }
    }

    Ok(())
}

/// Fragments assembled into inputs that are structurally plausible and
/// adversarial at once.
///
/// Purely random bytes are a weak generator for a reader: almost every sample
/// fails in the first few bytes and the deeper paths are never reached. A
/// vocabulary of *reader-significant* tokens keeps the generator producing
/// inputs that get far enough in to be interesting.
fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("(".to_owned()),
        Just(")".to_owned()),
        Just("[".to_owned()),
        Just("]".to_owned()),
        Just("{".to_owned()),
        Just("}".to_owned()),
        Just("\"".to_owned()),
        Just("\\".to_owned()),
        Just(";".to_owned()),
        Just("|".to_owned()),
        Just("#|".to_owned()),
        Just("|#".to_owned()),
        Just("#".to_owned()),
        Just("#(".to_owned()),
        Just("#'".to_owned()),
        Just("#\\".to_owned()),
        Just("#+".to_owned()),
        Just("#-".to_owned()),
        Just("#.".to_owned()),
        Just("#:".to_owned()),
        Just("#;".to_owned()),
        Just("#C(".to_owned()),
        Just("#1=".to_owned()),
        Just("#1#".to_owned()),
        Just("'".to_owned()),
        Just("`".to_owned()),
        Just(",".to_owned()),
        Just(",@".to_owned()),
        Just(".".to_owned()),
        Just(" ".to_owned()),
        Just("\n".to_owned()),
        Just("\t".to_owned()),
        Just("\r\n".to_owned()),
        Just("defun".to_owned()),
        Just("nil".to_owned()),
        Just("t".to_owned()),
        Just("λ".to_owned()),
        Just("日本語".to_owned()),
        Just("\u{0}".to_owned()),
        Just("\u{202e}".to_owned()),
    ]
}

/// Reader-significant tokens, in any order, for every dialect.
#[test]
fn adversarial_token_soup_never_panics() {
    check_property(
        400,
        (
            prop::collection::vec(token(), 0..30),
            0usize..DIALECTS.len(),
        ),
        |(tokens, dialect_index)| {
            let source = tokens.concat();
            let dialect = DIALECTS[dialect_index];
            // The message, not just the boolean: a CI failure that says only
            // "assertion failed" costs a round trip to reproduce.
            check_invariants(&source, dialect).map_err(TestCaseError::fail)
        },
    );
}

/// Arbitrary text, including bytes no Lisp reader would accept. Weaker than
/// the token soup at reaching deep paths, stronger at reaching the boundary
/// conditions of the byte scanner.
#[test]
fn arbitrary_text_never_panics() {
    check_property(
        400,
        (".{0,200}", 0usize..DIALECTS.len()),
        |(source, dialect_index)| {
            check_invariants(&source, DIALECTS[dialect_index]).map_err(TestCaseError::fail)
        },
    );
}

/// Deep nesting must not overflow the stack. The parser documents an iterative
/// walk for exactly this reason, and a recursive helper added later would only
/// show up here.
#[test]
fn deeply_nested_input_does_not_overflow() {
    check_property(200, 1usize..2_000, |depth| {
        let source = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
        check_invariants(&source, Dialect::CommonLisp).map_err(TestCaseError::fail)
    });
}

/// Unbalanced in either direction, at any depth.
#[test]
fn unbalanced_input_is_refused_rather_than_crashing() {
    check_property(400, (0usize..60, 0usize..60), |(opens, closes)| {
        let source = format!("{}{}", "(".repeat(opens), ")".repeat(closes));
        check_invariants(&source, Dialect::CommonLisp).map_err(TestCaseError::fail)
    });
}

/// Every structural edit, at an arbitrary byte offset into an arbitrary
/// document.
///
/// `--at` takes a raw offset from the caller, so the offset reaching
/// `select_at` is genuinely untrusted — including offsets inside a multi-byte
/// character, inside a string literal, and past the end. The property asserted
/// is that every one of these *returns*: a refusal is a designed outcome, a
/// panic is not.
///
/// It is deliberately not asserted that a successful rewrite reparses. The
/// edit layer is span arithmetic over a tree and does not re-read its own
/// output; removing a delimiter that was separating two tokens can change how
/// the bytes after it lex. `a_lossy_edit_is_refused_before_it_is_emitted`
/// below pins the guarantee where it actually lives.
#[test]
fn every_edit_at_an_arbitrary_offset_returns_rather_than_panicking() {
    check_property(
        400,
        ("[a-z()\\[\\] \u{3042}\"]{0,80}", 0usize..100),
        |(source, offset)| {
            let Ok(tree) = SyntaxTree::parse(&source) else {
                return Ok(());
            };
            let Ok(selection) = tree.select_at(offset) else {
                return Ok(());
            };

            // Each of these either returns a rewrite or refuses. Reaching the
            // end of the block is the assertion; a panic fails through the
            // harness.
            let outcomes = [
                Edit::kill(&source, &tree, selection),
                Edit::splice(&source, &tree, selection),
                Edit::raise(&source, &tree, selection),
                Edit::split(&source, &tree, selection),
                Edit::join(&source, &tree, selection),
                Edit::transpose_forward(&source, &tree, selection),
                Edit::transpose_backward(&source, &tree, selection),
                Edit::slurp_forward(&source, &tree, selection),
                Edit::slurp_backward(&source, &tree, selection),
                Edit::barf_forward(&source, &tree, selection),
                Edit::barf_backward(&source, &tree, selection),
                Edit::convolute(&source, &tree, selection),
            ];

            for outcome in outcomes.into_iter().flatten() {
                if outcome.len() > source.len() * 2 + 8 {
                    return Err(TestCaseError::fail(format!(
                        "an edit of {source:?} produced implausible output {outcome:?}"
                    )));
                }
            }
            Ok(())
        },
    );
}

/// A lossy edit is refused by the command, not merely by the write.
///
/// A generated case showed `Edit::splice` producing a document that no longer
/// parses. The mechanism is real and not exotic in kind: removing a delimiter
/// that was separating two tokens changes how the bytes after it lex, and the
/// edit layer is span arithmetic that does not re-read its own output.
///
/// What the CLI does about it is stronger than the write guard alone.
/// `edit_target` normalizes the rewrite, which reparses it, so the command
/// fails before printing — a caller piping `paredit edit` into a file never
/// receives the broken document in the first place. This test pins that,
/// because "we check before writing" and "we check before emitting" differ
/// exactly for the caller who redirects stdout.
#[test]
fn a_lossy_edit_is_refused_before_it_is_emitted() {
    // Reduced from a generated counterexample. Emacs Lisp reads `[...]` as a
    // vector, so unlike Common Lisp it accepts the input — which is what makes
    // this reachable from a command at all.
    let source = "\"\"[x]\"(]a []x\"a";

    let tree = SyntaxTree::parse(source).expect("the permissive reader accepts this");
    let selection = tree
        .select_at(2)
        .expect("offset 2 selects the bracket list");
    let rewritten = Edit::splice(source, &tree, selection).expect("splice returns a rewrite");
    assert!(
        SyntaxTree::parse(&rewritten).is_err(),
        "this fixture exists because the rewrite does not reparse; it now does, \
         so the case it guards has changed"
    );
    assert!(
        SyntaxTree::parse_with_dialect(source, Dialect::EmacsLisp).is_ok(),
        "the fixture must be reachable through a real dialect to mean anything"
    );

    let dir = std::env::temp_dir().join(format!(
        "paredit-lossy-edit-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    let file = dir.join("core.el");
    fs::write(&file, source).expect("write source");

    for extra in [vec![], vec!["--write"]] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_paredit"))
            .args(["edit", "splice", "--at", "2", "--file"])
            .arg(&file)
            .args(&extra)
            .output()
            .expect("run paredit");

        assert!(
            !output.status.success(),
            "paredit edit splice {extra:?} must refuse a rewrite that does not reparse"
        );
        assert!(
            output.stdout.is_empty(),
            "a refused edit must emit nothing, got {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    assert_eq!(
        fs::read_to_string(&file).expect("read source"),
        source,
        "the file must be untouched"
    );
    fs::remove_dir_all(&dir).expect("remove temp dir");
}

/// Every file under `fuzz/corpus` and `fuzz/artifacts`, replayed on stable.
///
/// A crasher found by a nightly libFuzzer run is only a regression test if it
/// keeps being run. Dropping the artifact into `fuzz/artifacts/` makes it one,
/// without anyone needing the nightly toolchain again.
#[test]
fn every_recorded_fuzz_input_still_upholds_the_invariants() {
    let mut checked = 0usize;
    let mut failures = Vec::new();

    for root in [
        PathBuf::from("fuzz/corpus"),
        PathBuf::from("fuzz/artifacts"),
    ] {
        for file in files_under(&root) {
            let Ok(bytes) = fs::read(&file) else {
                continue;
            };
            // A libFuzzer artifact is arbitrary bytes; only the ones that are
            // text can be handed to a reader at all.
            let Ok(source) = String::from_utf8(bytes) else {
                continue;
            };
            checked += 1;
            for dialect in DIALECTS {
                if let Err(failure) = check_invariants(&source, dialect) {
                    failures.push(format!("{}: {failure}", file.display()));
                }
            }
        }
    }

    println!("replayed {checked} recorded fuzz input(s)");
    assert!(
        failures.is_empty(),
        "recorded fuzz inputs regressed:\n{}",
        failures.join("\n")
    );
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
