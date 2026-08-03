//! A false-positive audit harness: runs every rule in this package over a
//! corpus of third-party Common Lisp and prints findings with their locations.
//!
//! Author-written tests encode the author's model of the language, not the
//! language. A sibling batch shipped ten rules that passed their own suites and
//! a sweep of this repository's fixtures; an audit over 3163 files nobody
//! involved had written produced 653 findings, killed one rule outright and
//! narrowed four more. So this exists, and it is `#[ignore]`d only because the
//! corpus is not checked in.
//!
//! ```text
//! PAREDIT_DECL_CORPUS=/path/to/MANIFEST.txt \
//!   cargo test -p paredit-feature-lint-type-declaration \
//!   -- --ignored --nocapture ignored_audit_corpus
//! ```
//!
//! The manifest is one absolute path per line. The output reports, per rule,
//! both the finding count **and the denominator** — how many files and how many
//! candidate occurrences the rule was actually offered. A zero-finding sweep
//! over zero candidates is a false clean and proves nothing at all.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::ENTRIES;

/// Counts the textual occurrences of each construct a rule keys on, so that a
/// finding count has a denominator to be read against.
fn candidate_counts(source: &str) -> [usize; 4] {
    let lower = source.to_ascii_lowercase();
    [
        lower.matches("(declare").count(),
        lower.matches("(declaim").count(),
        lower.matches("(the ").count(),
        lower.matches("&rest").count() + lower.matches("&body").count(),
    ]
}

#[test]
#[ignore = "needs a third-party corpus; set PAREDIT_DECL_CORPUS to a manifest"]
fn ignored_audit_corpus() {
    let Ok(manifest_path) = std::env::var("PAREDIT_DECL_CORPUS") else {
        panic!("set PAREDIT_DECL_CORPUS to a manifest of absolute .lisp paths");
    };
    let manifest = std::fs::read_to_string(&manifest_path).expect("read the manifest");

    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);

    let mut files_scanned = 0usize;
    let mut files_parsed = 0usize;
    let mut totals = [0usize; 4];
    let mut findings: Vec<(String, String, usize, String)> = Vec::new();

    for line in manifest.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        files_scanned += 1;
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        for (slot, count) in candidate_counts(&source).into_iter().enumerate() {
            totals[slot] += count;
        }
        let Ok(tree) = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp) else {
            continue;
        };
        files_parsed += 1;
        let Ok(outcomes) = collect_lint_outcomes(
            catalog,
            &index,
            Path::new(path),
            Dialect::CommonLisp,
            &tree,
            &source,
            RuleSelection::All,
        ) else {
            continue;
        };
        for outcome in outcomes {
            let (finding, _) = outcome.into_parts();
            let offset = finding.span.start().get();
            let line_number = source[..offset].matches('\n').count() + 1;
            let excerpt = source[offset..]
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .chars()
                .take(110)
                .collect::<String>();
            findings.push((
                finding.rule.to_owned(),
                path.to_owned(),
                line_number,
                excerpt,
            ));
        }
    }

    println!("\n===== DENOMINATOR =====");
    println!("files in manifest      : {files_scanned}");
    println!("files parsed as CL     : {files_parsed}");
    println!("(declare occurrences   : {}", totals[0]);
    println!("(declaim occurrences   : {}", totals[1]);
    println!("(the  occurrences      : {}", totals[2]);
    println!("&rest/&body occurrences: {}", totals[3]);

    println!("\n===== FINDINGS BY RULE =====");
    for entry in catalog.entries() {
        let name = entry.meta().name().as_str();
        let count = findings.iter().filter(|(rule, ..)| rule == name).count();
        println!("{count:>6}  {name}");
    }

    println!("\n===== EVERY FINDING =====");
    for (rule, path, line, excerpt) in &findings {
        println!("{rule}\t{path}:{line}\t{excerpt}");
    }
    println!("\ntotal findings: {}", findings.len());
}
