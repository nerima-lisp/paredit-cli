//! Every file in a corpus, through the parser, the formatter, and the selector.
//!
//! Section I item I10. The rest of the suite tests this tool against inputs
//! this tool's authors wrote, which is a systematic blind spot: a construct
//! nobody thought of is a construct nobody wrote a fixture for. A corpus test
//! closes it by asserting *invariants* rather than outputs, over code the
//! authors did not write.
//!
//! Four invariants, each of which holds regardless of what the file contains:
//!
//! 1. **Parsing terminates without panicking.** A parse *error* is fine — real
//!    Lisp carries reader macros this tool does not model — but a panic is a
//!    crash in a tool that agents run unattended.
//! 2. **Parsing is lossless.** `tree.source()` is the input, byte for byte. The
//!    whole edit layer rewrites spans of that string; a parser that dropped a
//!    byte would corrupt every rewrite built on it.
//! 3. **Formatting is idempotent.** `format(format(x)) == format(x)`. A
//!    formatter without this property produces diffs that never converge, which
//!    is worse than not formatting.
//! 4. **Every path the tree reports is selectable.** The tree hands out paths
//!    and the selector consumes them; a path the tree produced and the selector
//!    rejects is an internal contradiction.
//!
//! ## Where the corpus comes from
//!
//! `tests/fixtures/corpus` is vendored and always runs, so the invariants are
//! checked on every CI run without network access. It is small on purpose: it
//! holds the constructs that have historically been awkward, not a sample of
//! ordinary code.
//!
//! `PAREDIT_CORPUS_DIR` points at a real checkout — SBCL's source tree, a
//! Quicklisp dist, a company monorepo — for a run that means something at
//! scale. `scripts/fetch-corpus.sh` fetches one. Colon-separated for several.
//!
//! ```sh
//! PAREDIT_CORPUS_DIR=~/quicklisp/dists cargo test --test corpus -- --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use paredit_cli::dialect::Dialect;
use paredit_cli::sexpr::{ExpressionPath, Formatter, SyntaxTree};

/// Extensions the corpus walker treats as Lisp-family source.
///
/// Deliberately derived from `Dialect::from_extension` rather than listed, so a
/// dialect added to the tool is covered here without a second edit.
fn is_lisp_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(Dialect::from_extension)
        .is_some_and(|dialect| dialect != Dialect::Unknown)
}

/// Files that must not be read as source even when their extension matches.
///
/// A corpus checkout routinely contains build output and vendored copies; both
/// inflate the run without testing anything the originals do not.
fn is_skipped_directory(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "build" | "dist" | "vendor" | "node_modules" | ".paredit.cleanup"
    )
}

/// Every Lisp source file under `root`, sorted so a failure is reproducible.
///
/// Symlinks are not followed: a corpus checkout can contain a link to its own
/// parent, and the walk has to terminate.
fn corpus_files(root: &Path, budget: &mut usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if *budget == 0 {
                break;
            }
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !is_skipped_directory(&name) {
                    stack.push(path);
                }
            } else if metadata.is_file() && is_lisp_source(&path) {
                *budget -= 1;
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The corpus roots for this run.
fn corpus_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("tests/fixtures/corpus")];
    if let Ok(extra) = std::env::var("PAREDIT_CORPUS_DIR") {
        roots.extend(
            extra
                .split(':')
                .filter(|entry| !entry.trim().is_empty())
                .map(PathBuf::from),
        );
    }
    roots
}

/// How many files one run will read.
///
/// A bound rather than a sample: a corpus of a hundred thousand files would
/// turn `cargo test` into a batch job, and the point of this test is that it
/// runs every time. The count reached is printed, so a truncated run says so
/// instead of looking complete.
const MAX_CORPUS_FILES: usize = 4_000;

/// The largest file the corpus test will read, in bytes.
const MAX_CORPUS_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Default)]
struct Tally {
    files: usize,
    parsed: usize,
    parse_errors: usize,
    skipped_non_utf8: usize,
    skipped_large: usize,
}

/// Checks the four invariants on one file, returning failures rather than
/// panicking so one run reports every offender.
fn check_file(path: &Path, tally: &mut Tally) -> Vec<String> {
    let Ok(metadata) = fs::metadata(path) else {
        return Vec::new();
    };
    if metadata.len() > MAX_CORPUS_FILE_BYTES {
        tally.skipped_large += 1;
        return Vec::new();
    }
    let Ok(source) = fs::read(path) else {
        return Vec::new();
    };
    let Ok(source) = String::from_utf8(source) else {
        // Legacy Lisp sources are routinely Shift_JIS or Latin-1. Reading one
        // as UTF-8 is not this test's subject.
        tally.skipped_non_utf8 += 1;
        return Vec::new();
    };

    tally.files += 1;
    let dialect = Dialect::detect_in_source(Some(path), None, &source);

    // Invariant 1: parsing terminates. A panic here fails the test through the
    // harness rather than through an assertion, which is the correct outcome.
    let Ok(tree) = SyntaxTree::parse_with_dialect(&source, dialect) else {
        tally.parse_errors += 1;
        return Vec::new();
    };
    tally.parsed += 1;

    let mut failures = Vec::new();

    // Invariant 2: the parse is lossless. Every rewrite in this tool is a span
    // replacement over exactly this string.
    if tree.source() != source {
        failures.push(format!(
            "{}: the parsed source is not the input ({} bytes in, {} bytes retained)",
            path.display(),
            source.len(),
            tree.source().len()
        ));
    }

    // Invariant 3: formatting converges.
    let formatter = Formatter::with_dialect(2, dialect);
    let once = formatter.format(&tree);
    match SyntaxTree::parse_with_dialect(&once, dialect) {
        Ok(reparsed) => {
            let twice = formatter.format(&reparsed);
            if once != twice {
                failures.push(format!(
                    "{}: formatting is not idempotent; a second pass changed {} bytes",
                    path.display(),
                    once.len().abs_diff(twice.len()),
                ));
            }
        }
        Err(error) => failures.push(format!(
            "{}: formatted output does not reparse: {error}",
            path.display()
        )),
    }

    // Invariant 4: a path the tree produced resolves.
    for index in 0..tree.root_children().len() {
        let path_to_form = ExpressionPath::from_indexes(vec![index]);
        if tree.select_path(&path_to_form).is_err() {
            failures.push(format!(
                "{}: top-level path {index} is reported by the tree and rejected by the selector",
                path.display()
            ));
        }
    }

    failures
}

#[test]
fn every_corpus_file_upholds_the_parser_and_formatter_invariants() {
    let mut budget = MAX_CORPUS_FILES;
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let mut roots_seen = Vec::new();

    for root in corpus_roots() {
        if !root.is_dir() {
            // A missing `PAREDIT_CORPUS_DIR` entry is the caller's typo and
            // worth saying so; a missing vendored corpus is a broken checkout.
            assert!(
                root != Path::new("tests/fixtures/corpus"),
                "the vendored corpus at tests/fixtures/corpus is missing"
            );
            println!("corpus root {} does not exist; skipping", root.display());
            continue;
        }
        roots_seen.push(root.display().to_string());
        for file in corpus_files(&root, &mut budget) {
            failures.extend(check_file(&file, &mut tally));
        }
    }

    println!(
        "corpus roots: {}\n  files read      {}\n  parsed          {}\n  parse errors    {}\n  \
         skipped non-UTF-8 {}\n  skipped large   {}",
        roots_seen.join(", "),
        tally.files,
        tally.parsed,
        tally.parse_errors,
        tally.skipped_non_utf8,
        tally.skipped_large,
    );
    if budget == 0 {
        println!(
            "  NOTE: stopped at the {MAX_CORPUS_FILES}-file ceiling; \
             this run did not cover the whole corpus"
        );
    }

    assert!(
        tally.files > 0,
        "the corpus produced no readable files; the vendored fixtures are missing or empty"
    );
    assert!(
        failures.is_empty(),
        "{} corpus invariant failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The vendored corpus exists to be awkward. A run in which most of it fails to
/// parse would mean the fixtures had drifted into testing the error path.
#[test]
fn the_vendored_corpus_is_mostly_parseable() {
    let root = PathBuf::from("tests/fixtures/corpus");
    let mut budget = MAX_CORPUS_FILES;
    let mut tally = Tally::default();

    for file in corpus_files(&root, &mut budget) {
        let _ = check_file(&file, &mut tally);
    }

    assert!(
        tally.files >= 3,
        "the vendored corpus is too small to mean anything"
    );
    assert_eq!(
        tally.parse_errors, 0,
        "every vendored corpus file is expected to parse; \
         a file that should not parse belongs in a parser test, not here"
    );
}
