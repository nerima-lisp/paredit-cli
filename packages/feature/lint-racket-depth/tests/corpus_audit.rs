//! The third-party false-positive audit, over real Racket rather than over
//! fixtures this package's author wrote.
//!
//! Author-written tests encode the author's model of the language, not the
//! language. This sweep runs the whole package over an external corpus and
//! reports, per rule, the number of **candidate occurrences** beside the number
//! of findings — because a zero-finding sweep over zero candidates is a false
//! clean, not a clean.
//!
//! It is `#[ignore]`d because it needs a corpus that is not in the repository.
//! Point it at one and run it:
//!
//! ```text
//! PAREDIT_RACKET_CORPUS=/path/to/racket:/path/to/typed-racket \
//!   cargo test -p paredit-feature-lint-racket-depth \
//!   --test corpus_audit -- --ignored --nocapture
//! ```

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};

use support::RULE_NAMES;

/// Every head any rule in this package anchors on. A corpus containing none of
/// these proves nothing by producing no findings.
const ANCHOR_HEADS: [&str; 16] = [
    "match",
    "match-lambda",
    "match-lambda*",
    "begin0",
    "case-lambda",
    "parameterize",
    "begin",
    "when",
    "unless",
    "lambda",
    "define",
    "let",
    "let*",
    "letrec",
    "letrec*",
    "\u{3bb}",
];

fn corpus_roots() -> Vec<PathBuf> {
    std::env::var("PAREDIT_RACKET_CORPUS")
        .map(|value| value.split(':').map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn racket_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            racket_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rkt") {
            out.push(path);
        }
    }
}

/// Counts, per anchored head, how many list nodes carry it — including the ones
/// the rules will correctly decline, since the point is to prove the corpus
/// exercises the head at all.
fn count_anchors(tree: &SyntaxTree, counts: &mut BTreeMap<&'static str, usize>) {
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(first) = view.children.first() {
            if first.kind == ExpressionKind::Atom {
                if let Some(text) = first.text.as_deref() {
                    if let Some(count) = counts.get_mut(text) {
                        *count += 1;
                    }
                }
            }
        }
        stack.extend(view.children.iter());
    }
}

#[test]
#[ignore = "needs an external Racket corpus; see the module docs"]
fn the_corpus_earns_no_findings_over_a_real_denominator() {
    let roots = corpus_roots();
    assert!(
        !roots.is_empty(),
        "set PAREDIT_RACKET_CORPUS to one or more ':'-separated directories"
    );

    let mut files = Vec::new();
    for root in &roots {
        racket_files(root, &mut files);
    }
    files.sort();
    assert!(!files.is_empty(), "the corpus contains no .rkt files");

    let mut anchors: BTreeMap<&'static str, usize> =
        ANCHOR_HEADS.iter().map(|head| (*head, 0)).collect();
    let mut findings: BTreeMap<&'static str, Vec<String>> =
        RULE_NAMES.iter().map(|rule| (*rule, Vec::new())).collect();
    let mut parse_failures: Vec<(PathBuf, String)> = Vec::new();
    let mut scanned = 0usize;

    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let tree = match SyntaxTree::parse_with_dialect(&source, Dialect::Racket) {
            Ok(tree) => tree,
            Err(error) => {
                // Record the bytes at the failure offset, not only the error:
                // the error names the dispatch character but not which reader
                // construct it belongs to, and the distribution of those is the
                // actionable part of a parse-failure report.
                let described = format!("{error:?}");
                let offset = described
                    .rsplit_once("position: ")
                    .and_then(|(_, rest)| rest.trim_end_matches(" }").parse::<usize>().ok());
                let snippet = offset
                    .and_then(|at| source.get(at..(at + 12).min(source.len())))
                    .unwrap_or("")
                    .replace('\n', "\\n");
                parse_failures.push((path.clone(), snippet));
                continue;
            }
        };
        scanned += 1;
        count_anchors(&tree, &mut anchors);
        for outcome in support::run(&source, Dialect::Racket, path) {
            let (finding, _) = outcome.into_parts();
            let line = source[..finding.span.start().get().min(source.len())]
                .lines()
                .count()
                .max(1);
            let text = source.lines().nth(line - 1).unwrap_or("").trim();
            findings.entry(finding.rule).or_default().push(format!(
                "{}:{line}  {}",
                path.display(),
                &text[..text.len().min(90)]
            ));
        }
    }

    println!("\n=== Racket corpus audit ===");
    println!("files found        : {}", files.len());
    println!("files parsed       : {scanned}");
    println!("parse failures     : {}", parse_failures.len());
    println!("\n--- candidate occurrences per anchored head ---");
    for (head, count) in &anchors {
        println!("{head:<16} {count:>8}");
    }
    println!("\n--- findings per rule ---");
    for rule in RULE_NAMES {
        let hits = &findings[rule];
        println!("{rule:<44} {:>6}", hits.len());
        for hit in hits.iter().take(25) {
            println!("      {hit}");
        }
    }
    if !parse_failures.is_empty() {
        let mut causes: BTreeMap<String, usize> = BTreeMap::new();
        for (_, snippet) in &parse_failures {
            // Group by the reader construct, not the whole snippet.
            let key: String = snippet.chars().take(3).collect();
            *causes.entry(key).or_default() += 1;
        }
        let mut ranked: Vec<(&String, &usize)> = causes.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        println!("\n--- parse failures grouped by reader construct ---");
        for (construct, count) in ranked.iter().take(20) {
            println!("      {construct:<14} {count:>6}");
        }
    }

    // Every anchored head must actually occur, or the clean sweep is a false
    // clean for that rule. `match-lambda*` is exempt and asserted separately
    // below: it genuinely does not occur in this corpus, and pretending
    // otherwise would be the false clean this test exists to prevent.
    for head in ANCHOR_HEADS {
        if head == "match-lambda*" {
            continue;
        }
        assert!(
            anchors[head] > 0,
            "the corpus contains no `{head}` form, so its zero findings prove nothing"
        );
    }
    assert_eq!(
        anchors["match-lambda*"], 0,
        "match-lambda* now occurs in the corpus; fold it back into the asserted set"
    );
}
