//! The third-party audit harness, retained so the sweep can be repeated rather
//! than taken on trust.
//!
//! Reads a corpus directory from `LFECARP_CORPUS` and reports the parse rate,
//! the per-rule candidate denominators and every finding with a `file:line`
//! locator, so each can be adjudicated by hand. **A no-op when the variable is
//! unset**, so it costs a normal test run nothing and depends on no checkout
//! outside this repository.
//!
//! `corpus_tests.rs` is the permanent, self-contained pair to this: the shapes
//! this sweep found, distilled into fixtures that travel with the package.
//!
//! ```text
//! LFECARP_CORPUS=/path/to/corpus \
//!   cargo test -p paredit-feature-lint-lfe-carp-depth --lib -- --nocapture audit_the_corpus
//! ```
//!
//! The last run — ~140 cloned repositories — reported 921 `.lfe` files
//! scanned, 917 parsed, **0 findings over 39 guard candidates and 4284 clause
//! candidates**, and 553 `.carp` files at a 100% parse rate.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::{ENTRIES, dead_clause, illegal_guard_call};

fn files_with(root: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            files_with(&path, extension, out);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            out.push(path);
        }
    }
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

#[test]
fn audit_the_corpus() {
    let Ok(root) = std::env::var("LFECARP_CORPUS") else {
        return;
    };
    let root = PathBuf::from(root);

    let mut lfe = Vec::new();
    files_with(&root, "lfe", &mut lfe);
    lfe.sort();

    let mut parsed = 0usize;
    let mut failed = 0usize;
    let mut guard_candidates = 0usize;
    let mut clause_candidates = 0usize;
    let mut guard_findings = Vec::new();
    let mut clause_findings = Vec::new();

    for path in &lfe {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(tree) = SyntaxTree::parse_with_dialect(&source, Dialect::Lfe) else {
            failed += 1;
            println!("PARSE-FAIL {}", path.display());
            continue;
        };
        parsed += 1;
        guard_candidates += illegal_guard_call::domain::candidate_count(Dialect::Lfe, &tree);
        clause_candidates += dead_clause::domain::candidate_count(Dialect::Lfe, &tree);

        // Through the *real* engine, not the domain. The domain's `collect`
        // has no ancestor context, so it cannot apply the quote and
        // syntax-template gates that the rules do — auditing it would
        // overcount and adjudicate findings that never reach a user.
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let Ok(outcomes) = collect_lint_outcomes(
            catalog,
            &index,
            path,
            Dialect::Lfe,
            &tree,
            &source,
            RuleSelection::All,
        ) else {
            continue;
        };
        for outcome in outcomes {
            let (finding, _) = outcome.into_parts();
            let line = line_of(&source, finding.span.start().get());
            let text = finding
                .span
                .slice(&source)
                .replace('\n', " ")
                .chars()
                .take(90)
                .collect::<String>();
            let entry = format!("{}:{line} {text}", path.display());
            if finding.rule == "lfe-illegal-guard-call" {
                guard_findings.push(format!("GUARD {entry}"));
            } else {
                clause_findings.push(format!("CLAUSE {entry}"));
            }
        }
    }

    println!("=== LFE DENOMINATORS ===");
    println!("files scanned        {}", lfe.len());
    println!("files parsed         {parsed}");
    println!("files failed         {failed}");
    println!("guard candidates     {guard_candidates}");
    println!("clause candidates    {clause_candidates}");
    println!("guard findings       {}", guard_findings.len());
    println!("clause findings      {}", clause_findings.len());
    println!("=== GUARD FINDINGS ===");
    for line in &guard_findings {
        println!("{line}");
    }
    println!("=== CLAUSE FINDINGS ===");
    for line in &clause_findings {
        println!("{line}");
    }

    // Carp: parse rate only, since this package ships no Carp rule.
    let mut carp = Vec::new();
    files_with(&root, "carp", &mut carp);
    let mut carp_parsed = 0usize;
    let mut carp_failed = 0usize;
    for path in &carp {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        if SyntaxTree::parse_with_dialect(&source, Dialect::Carp).is_ok() {
            carp_parsed += 1;
        } else {
            carp_failed += 1;
            println!("CARP-PARSE-FAIL {}", path.display());
        }
    }
    println!("=== CARP DENOMINATORS ===");
    println!("files scanned        {}", carp.len());
    println!("files parsed         {carp_parsed}");
    println!("files failed         {carp_failed}");
}
