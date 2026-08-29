//! The permanent pair: realistic *correct* Hy that earns nothing, and its
//! dangerous twin that earns exactly one finding per shape.
//!
//! Both halves are needed and neither is sufficient. A clean file alone passes
//! for a rule that never fires; a dangerous file alone passes for a rule that
//! fires on everything. And the clean half asserts a **non-zero candidate
//! count**, because a clean sweep over a file with no multi-clause `try` would
//! prove nothing — that is the false-clean this project has been bitten by.

mod support;

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, SyntaxTree};

use support::rules_fired;

const CLEAN: &str = include_str!("fixtures/clean.hy");
const DANGEROUS: &str = include_str!("fixtures/dangerous.hy");

fn head_of(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List
        || view.delimiter != Some(Delimiter::Paren)
        || !view.reader_prefixes.is_empty()
    {
        return None;
    }
    let first = view.children.first()?;
    (first.kind == ExpressionKind::Atom)
        .then_some(first.text.as_deref())
        .flatten()
}

/// `try` forms carrying two or more `except` clauses — the only ones on which
/// a finding is even possible, and so the only honest denominator.
fn multi_clause_try_count(source: &str) -> usize {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Hy).expect("parse fixture");
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    let mut count = 0;
    while let Some(view) = stack.pop() {
        if head_of(view) == Some("try") {
            let clauses = view
                .children
                .iter()
                .skip(1)
                .filter(|child| head_of(child) == Some("except"))
                .count();
            if clauses >= 2 {
                count += 1;
            }
        }
        stack.extend(view.children.iter());
    }
    count
}

fn except_binding(clause: &str) -> &str {
    let mut depth = 0;
    for (index, character) in clause.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return clause[..=index].trim();
                }
            }
            _ => {}
        }
    }
    clause.lines().next().unwrap_or("").trim()
}

#[test]
fn the_clean_fixture_parses_and_offers_real_candidates() {
    // Without this the next test is a false clean: a fixture the reader
    // refused, or one with no handler chain in it, reports nothing for reasons
    // that have nothing to do with the rule being correct.
    assert!(
        multi_clause_try_count(CLEAN) >= 7,
        "clean.hy must contain handler chains for the rule to judge, found {}",
        multi_clause_try_count(CLEAN)
    );
}

#[test]
fn realistic_correct_hy_earns_no_findings() {
    assert_eq!(
        rules_fired(CLEAN, Dialect::Hy),
        Vec::<&str>::new(),
        "clean.hy must be clean"
    );
}

#[test]
fn the_dangerous_twin_fires_once_per_shape() {
    // Eight shapes, each contributing exactly one finding, except `bare-first`
    // which kills both of the clauses that follow it.
    assert_eq!(
        rules_fired(DANGEROUS, Dialect::Hy).len(),
        9,
        "each dangerous shape must fire exactly once; bare-first kills two"
    );
    assert!(
        rules_fired(DANGEROUS, Dialect::Hy)
            .iter()
            .all(|rule| *rule == "hy-unreachable-except-clause")
    );
}

/// Which `try` each finding lands in, so that a rule reporting the right total
/// for the wrong reasons fails here.
#[test]
fn each_dangerous_shape_is_reported_at_its_own_clause() {
    let findings = support::run(DANGEROUS, Dialect::Hy, Path::new("dangerous.hy"));
    let mut reported: Vec<&str> = findings
        .into_iter()
        .map(|outcome| {
            let (finding, _) = outcome.into_parts();
            let start = finding.span.start().get();
            let end = finding.span.end().get().min(DANGEROUS.len());
            except_binding(&DANGEROUS[start..end])
        })
        .collect();
    reported.sort_unstable();
    assert_eq!(
        reported,
        vec![
            "(except [e AppError]",
            "(except [e IOError]",
            "(except [e KeyError]",
            "(except [e ModuleNotFoundError]",
            "(except [e OSError]",
            "(except [e UnicodeDecodeError]",
            "(except [e ValueError]",
            "(except [e ValueError]",
            "(except [e [KeyError IndexError]]",
        ]
    );
}
