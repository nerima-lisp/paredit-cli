//! The third-party false-positive audit, over real Hy rather than over
//! fixtures this package's author wrote.
//!
//! Author-written tests encode the author's model of the language, not the
//! language. This sweep runs the package over an external corpus and reports,
//! per rule, the number of **candidate occurrences** beside the number of
//! findings — because a zero-finding sweep over zero candidates is a false
//! clean, not a clean.
//!
//! For this rule the candidate count that matters is not "how many `try`
//! forms" but **how many `try` forms have two or more `except` clauses**: a
//! `try` with one clause can never earn a finding, so counting those would
//! inflate the denominator into meaninglessness.
//!
//! `#[ignore]`d because it needs a corpus that is not in the repository:
//!
//! ```text
//! PAREDIT_HY_CORPUS=/path/to/hy:/path/to/hyrule \
//!   cargo test -p paredit-feature-lint-hy-depth \
//!   --test corpus_audit -- --ignored --nocapture
//! ```

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, SyntaxTree};

use support::RULE_NAMES;

fn corpus_roots() -> Vec<PathBuf> {
    std::env::var("PAREDIT_HY_CORPUS")
        .map(|value| value.split(':').map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn hy_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // `.git` holds packed objects, not source.
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            hy_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "hy") {
            out.push(path);
        }
    }
}

/// What the corpus offers this rule, counted honestly.
#[derive(Debug, Default)]
struct Candidates {
    /// Every `(try …)` form, however many clauses it has.
    try_forms: usize,
    /// `try` forms carrying two or more `except` clauses — the only ones on
    /// which a finding is even possible.
    try_with_two_or_more_clauses: usize,
    /// Every `(except …)` clause.
    except_clauses: usize,
    /// `(except [] …)`, the bare catch-everything.
    bare_except_clauses: usize,
    /// Clauses naming a tuple of types, the shape most likely to be misread.
    tuple_type_clauses: usize,
}

fn is_call(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && view.reader_prefixes.is_empty()
}

fn head_of(view: &ExpressionView) -> Option<&str> {
    if !is_call(view) {
        return None;
    }
    let first = view.children.first()?;
    (first.kind == ExpressionKind::Atom)
        .then_some(first.text.as_deref())
        .flatten()
}

fn count_candidates(tree: &SyntaxTree, counts: &mut Candidates) {
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        match head_of(view) {
            Some("try") => {
                counts.try_forms += 1;
                let clauses = view
                    .children
                    .iter()
                    .skip(1)
                    .filter(|child| head_of(child) == Some("except"))
                    .count();
                if clauses >= 2 {
                    counts.try_with_two_or_more_clauses += 1;
                }
            }
            Some("except") => {
                counts.except_clauses += 1;
                if let Some(bindings) = view.children.get(1) {
                    if bindings.delimiter == Some(Delimiter::Bracket) {
                        if bindings.children.is_empty() {
                            counts.bare_except_clauses += 1;
                        }
                        if bindings
                            .children
                            .iter()
                            .any(|child| child.delimiter == Some(Delimiter::Bracket))
                        {
                            counts.tuple_type_clauses += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        stack.extend(view.children.iter());
    }
}

#[test]
#[ignore = "needs an external Hy corpus; see the module docs"]
fn the_corpus_earns_no_findings_over_a_real_denominator() {
    let roots = corpus_roots();
    assert!(
        !roots.is_empty(),
        "set PAREDIT_HY_CORPUS to one or more ':'-separated directories"
    );

    let mut files = Vec::new();
    for root in &roots {
        hy_files(root, &mut files);
    }
    files.sort();
    files.dedup();
    assert!(!files.is_empty(), "the corpus contains no .hy files");

    let mut candidates = Candidates::default();
    let mut findings: BTreeMap<&'static str, Vec<String>> =
        RULE_NAMES.iter().map(|rule| (*rule, Vec::new())).collect();
    let mut parse_failures: Vec<(PathBuf, String, String)> = Vec::new();
    let mut scanned = 0usize;
    let mut unreadable = 0usize;

    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            unreadable += 1;
            continue;
        };
        let tree = match SyntaxTree::parse_with_dialect(&source, Dialect::Hy) {
            Ok(tree) => tree,
            Err(error) => {
                // Record the bytes at the failure offset, not only the error:
                // the error names the dispatch character but not which reader
                // construct it belongs to, and the distribution of those is
                // the actionable part of a parse-failure report.
                let described = format!("{error:?}");
                let offset = described
                    .rsplit_once("position: ")
                    .and_then(|(_, rest)| rest.trim_end_matches(" }").parse::<usize>().ok());
                let snippet = offset
                    .and_then(|at| source.get(at..(at + 24).min(source.len())))
                    .unwrap_or("")
                    .replace('\n', "\\n");
                let kind = described
                    .split_once(' ')
                    .map_or(described.clone(), |(head, _)| head.to_owned());
                parse_failures.push((path.clone(), kind, snippet));
                continue;
            }
        };
        scanned += 1;
        count_candidates(&tree, &mut candidates);
        for outcome in support::run(&source, Dialect::Hy, path) {
            let (finding, _) = outcome.into_parts();
            let line = source[..finding.span.start().get().min(source.len())]
                .lines()
                .count()
                .max(1);
            let text = source.lines().nth(line - 1).unwrap_or("").trim();
            findings.entry(finding.rule).or_default().push(format!(
                "{}:{line}  {}",
                path.display(),
                &text[..text.len().min(100)]
            ));
        }
    }

    println!("\n=== Hy corpus audit ===");
    println!("files found          : {}", files.len());
    println!("files unreadable     : {unreadable}");
    println!("files parsed         : {scanned}");
    println!("parse failures       : {}", parse_failures.len());
    let rate = scanned as f64 / (files.len() - unreadable).max(1) as f64 * 100.0;
    println!("parse rate           : {rate:.2}%");

    println!("\n--- candidate occurrences ---");
    println!(
        "try forms                        {:>8}",
        candidates.try_forms
    );
    println!(
        "try with >= 2 except clauses     {:>8}   <- the only ones that can earn a finding",
        candidates.try_with_two_or_more_clauses
    );
    println!(
        "except clauses                   {:>8}",
        candidates.except_clauses
    );
    println!(
        "bare (except [] ...) clauses     {:>8}",
        candidates.bare_except_clauses
    );
    println!(
        "clauses naming a tuple of types  {:>8}",
        candidates.tuple_type_clauses
    );

    println!("\n--- findings per rule ---");
    for rule in RULE_NAMES {
        let hits = &findings[rule];
        println!("{rule:<44} {:>6}", hits.len());
        for hit in hits {
            println!("      {hit}");
        }
    }

    if !parse_failures.is_empty() {
        let mut causes: BTreeMap<String, (usize, String, PathBuf)> = BTreeMap::new();
        for (path, kind, snippet) in &parse_failures {
            let key: String = format!("{kind} @ {}", snippet.chars().take(4).collect::<String>());
            let entry = causes
                .entry(key)
                .or_insert_with(|| (0, snippet.clone(), path.clone()));
            entry.0 += 1;
        }
        let mut ranked: Vec<_> = causes.iter().collect();
        ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1.0));
        println!("\n--- parse failures grouped by reader construct ---");
        for (cause, (count, snippet, example)) in ranked {
            println!(
                "      {count:>5}  {cause:<28} {snippet:<28} {}",
                example.display()
            );
        }
        // Every failing path, so the list can be fed to Hy's own reader as an
        // oracle. A file this reader refuses that Hy accepts is a reader gap;
        // one they both refuse is simply not Hy.
        println!("\n--- every parse failure, one path per line ---");
        for (path, _, _) in &parse_failures {
            println!("PARSEFAIL {}", path.display());
        }
    }

    // The denominator assertions. A clean sweep over no candidates is a false
    // clean, and this is the one place that can catch it.
    assert!(
        candidates.try_forms > 0,
        "the corpus contains no `try` form, so a clean sweep proves nothing"
    );
    assert!(
        candidates.try_with_two_or_more_clauses > 0,
        "no `try` in the corpus has two or more `except` clauses, so this rule was never \
         given an opportunity to fire and its zero findings prove nothing"
    );
}
