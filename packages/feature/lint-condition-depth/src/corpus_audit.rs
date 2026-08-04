//! The third-party false-positive audit: both rules run over real Common Lisp
//! that nobody here wrote.
//!
//! `#[ignore]`d, because it reads corpora that only exist on a machine with SBCL
//! and Quicklisp installed. Point it at them and run:
//!
//! ```text
//! CD_CORPUS_ROOTS=/nix/store/…-sbcl-2.6.0/lib/sbcl/src:$HOME/quicklisp/dists \
//!   cargo test -p paredit-feature-lint-condition-depth --lib \
//!   -- --ignored --nocapture corpus_audit
//! ```
//!
//! # The denominator is the point
//!
//! A zero-finding sweep over zero candidates is a **false-clean**, and it is the
//! way this kind of audit usually fails: a harness that errors out, or a glob
//! that matches nothing, reports zero findings and looks like success. So the
//! report prints, per rule, the number of nodes the dispatcher actually handed
//! it — and [`corpus_audit_self_test`] runs first, over a known-dirty source,
//! and **fails loudly if the harness cannot find a defect it was told is there**.
//! Read that line before believing any zero below it.
//!
//! # The result, 2026-08-04
//!
//! SBCL 2.6.0's own sources (`lib/sbcl`, which ships `src/` and `contrib/`) and
//! the installed Quicklisp dist, 38 systems:
//!
//! ```text
//! files scanned      : 1619   (898 SBCL + 721 Quicklisp)
//! files unparsed     :   31
//! bytes              : 28,378,755
//!
//! DENOMINATOR — candidate nodes handed to each rule:
//!   condition-type-datum-with-string-initarg     2521
//!   unwind-protect-cleanup-signals                184
//!
//! FINDINGS: 0
//! ```
//!
//! **Zero findings over 2,705 candidate nodes**, so there was nothing to
//! adjudicate. Three things keep that honest:
//!
//! 1. The self-test passed in the same run (`2/2 planted defects found`).
//! 2. **End-to-end validation.** Two defects were planted into a real SBCL
//!    source file (`src/assembly/assemfile.lisp`) and the sweep found exactly
//!    those two and nothing else, with a second untouched SBCL file
//!    (`src/code/restart.lisp`) in the same directory reporting zero. The sweep
//!    can therefore see a defect embedded in the corpus's own idiom, not only in
//!    a source written here.
//! 3. **The 31 unparsed files are reported, not hidden.** They mention roughly
//!    134 heads of interest between them — about 5% of the corpus total — so
//!    coverage is ~95%, and the remainder is named file by file in the output.
//!    They fail on reader macros this parser does not model (`sb-cover`,
//!    `src/code/reader.lisp`, `iterate`, `mgl-pax`), not on anything these rules
//!    would have judged.
//!
//! # What zero *true* positives means
//!
//! Stated plainly: the sweep found no real defects either. That is the expected
//! result for mature, widely-deployed libraries — the odd-length spelling fails
//! loudly the first time it runs, so it does not survive to a release — and it
//! is not evidence that the rules are useless. What it *is* evidence for is a
//! **false-positive rate of 0 over 2,705 candidates**, which is the bar this
//! audit exists to measure. These rules land on code under development, which is
//! not what a distribution corpus contains.

use std::fs;
use std::path::{Path, PathBuf};

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

static ENTRIES: [RuleEntry; 2] = [
    RuleEntry::new(
        &crate::condition_type_datum_with_string_initarg::META,
        &crate::condition_type_datum_with_string_initarg::RULE,
    ),
    RuleEntry::new(
        &crate::unwind_protect_cleanup_signals::META,
        &crate::unwind_protect_cleanup_signals::RULE,
    ),
];

/// A source with one of each defect, used to prove the harness works before any
/// zero is believed.
const KNOWN_DIRTY: &str = r#"
(define-condition store-error (error) ((key :initarg :key)) (:report "e"))
(defun fetch (store key)
  (error 'store-error "no entry for ~S" key))
(defun copy (in out)
  (unwind-protect (transfer in out)
    (unless (complete-p out) (error "transfer incomplete"))))
"#;

#[derive(Default)]
struct Totals {
    files_scanned: usize,
    files_unparsed: usize,
    bytes: u64,
    /// `(rule, candidate nodes handed to it)`
    candidates: Candidates,
    findings: Vec<String>,
    /// Files the reader refused, with the count of condition-system heads their
    /// raw text mentions. A file that does not parse is **invisible** to this
    /// audit, so its share of the corpus has to be reported rather than
    /// silently dropped.
    unparsed: Vec<(PathBuf, usize)>,
}

/// A crude count of the heads these rules anchor on, from raw text — used only
/// to say how much a file that failed to parse would have contributed.
fn mentions_of_interest(source: &str) -> usize {
    let lower = source.to_ascii_lowercase();
    [
        "(unwind-protect",
        "(error ",
        "(cerror ",
        "(signal ",
        "(warn ",
        "(make-condition ",
    ]
    .iter()
    .map(|needle| lower.matches(needle).count())
    .sum()
}

/// How many nodes the dispatcher handed each rule, by rule name.
type Candidates = Vec<(&'static str, u64)>;

/// Lints one source, returning `(candidates, findings)`.
fn lint(path: &Path, source: &str) -> Option<(Candidates, Vec<String>)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).ok()?;
    let outcome = collect_lint_pass(
        catalog,
        &index,
        path,
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .ok()?;

    let candidates = outcome
        .timings
        .as_ref()
        .expect("measure: true produces timings")
        .entries()
        .map(|(position, _, invocations)| {
            (
                catalog.entries()[position].meta().name().as_str(),
                invocations,
            )
        })
        .collect();

    let findings = outcome
        .outcomes
        .into_iter()
        .map(|item| {
            let (finding, _) = item.into_parts();
            let line = source[..finding.span.start().get()].lines().count();
            let text = source
                .get(finding.span.start().get()..finding.span.end().get())
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            format!("{}:{line}: [{}] {text}", path.display(), finding.rule)
        })
        .collect();

    Some((candidates, findings))
}

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "lisp" | "cl" | "asd" | "lsp"))
        {
            out.push(path);
        }
    }
}

fn accumulate(totals: &mut Totals, path: &Path) {
    let Ok(source) = fs::read_to_string(path) else {
        return;
    };
    totals.files_scanned += 1;
    totals.bytes += source.len() as u64;
    let Some((candidates, findings)) = lint(path, &source) else {
        totals.files_unparsed += 1;
        totals
            .unparsed
            .push((path.to_path_buf(), mentions_of_interest(&source)));
        return;
    };
    if totals.candidates.is_empty() {
        totals.candidates = candidates.iter().map(|(name, _)| (*name, 0)).collect();
    }
    for (name, count) in candidates {
        if let Some(slot) = totals
            .candidates
            .iter_mut()
            .find(|(existing, _)| *existing == name)
        {
            slot.1 += count;
        }
    }
    totals.findings.extend(findings);
}

/// Proves the harness can find a defect it was told is there.
///
/// Runs unattended, unlike the sweep itself: it needs no corpus. If this is
/// green and the sweep reports zero, the zero means something.
#[test]
fn corpus_audit_self_test() {
    let (candidates, findings) =
        lint(Path::new("known-dirty.lisp"), KNOWN_DIRTY).expect("the known-dirty source parses");
    assert_eq!(
        findings.len(),
        2,
        "the harness must find both planted defects, got: {findings:?}"
    );
    for (rule, count) in &candidates {
        assert!(
            *count > 0,
            "{rule} must be handed candidates by the known-dirty source"
        );
    }
}

#[test]
#[ignore = "needs CD_CORPUS_ROOTS pointing at SBCL and Quicklisp sources"]
fn corpus_audit() {
    // Self-test first, and loudly. A harness that silently fails looks exactly
    // like a clean sweep.
    corpus_audit_self_test();
    println!("harness self-test: OK (2/2 planted defects found)");

    let roots = std::env::var("CD_CORPUS_ROOTS").expect("set CD_CORPUS_ROOTS");
    let mut totals = Totals::default();
    let mut per_root = Vec::new();

    for root in roots.split(':').filter(|root| !root.is_empty()) {
        let mut files = Vec::new();
        collect_files(Path::new(root), &mut files);
        files.sort();
        let before = totals.files_scanned;
        let findings_before = totals.findings.len();
        for path in &files {
            accumulate(&mut totals, path);
        }
        per_root.push((
            root.to_owned(),
            totals.files_scanned - before,
            totals.findings.len() - findings_before,
        ));
    }

    println!("\n=== CORPUS AUDIT ===");
    for (root, files, findings) in &per_root {
        println!("  root {root}\n    files={files} findings={findings}");
    }
    println!(
        "\n  files scanned      : {}\n  files unparsed     : {}\n  bytes              : {}",
        totals.files_scanned, totals.files_unparsed, totals.bytes
    );
    println!("\n  DENOMINATOR — candidate nodes handed to each rule:");
    for (rule, count) in &totals.candidates {
        println!("    {rule:44} {count}");
    }
    let missed: usize = totals.unparsed.iter().map(|(_, count)| count).sum();
    println!(
        "\n  UNPARSED FILES: {} (invisible to this audit; they mention ~{missed} heads of \
         interest between them)",
        totals.unparsed.len()
    );
    for (path, count) in &totals.unparsed {
        println!("    {count:4} heads  {}", path.display());
    }

    println!("\n  FINDINGS: {}", totals.findings.len());
    for finding in &totals.findings {
        println!("    {finding}");
    }

    assert!(
        totals.files_scanned > 0,
        "the corpus roots matched no files at all — that is a harness failure, not a clean sweep"
    );
    for (rule, count) in &totals.candidates {
        assert!(
            *count > 0,
            "{rule} was handed zero candidates; a zero-finding sweep over zero candidates \
             proves nothing"
        );
    }
}
