//! The permanent corpus test: realistic *correct* Racket earns nothing, and its
//! dangerous twin earns one finding per rule.
//!
//! The clean half alone would pass for the wrong reason if the rules simply
//! never matched anything, so it also asserts that the corpus really holds the
//! shapes the rules anchor on — a non-zero *candidate* count per head, counted
//! independently of whether any rule fired. The dangerous half then proves each
//! rule can still fire on those same shapes.
//!
//! Both fixtures compile under `raco make` with no output at all (Racket v9.2),
//! which is the point of the dangerous one: every defect in it is invisible to
//! the toolchain.

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};

use support::{RULE_NAMES, rules_fired};

/// The heads the rules anchor on. A corpus containing none of these proves
/// nothing by producing no findings.
const ANCHOR_HEADS: [&str; 12] = [
    "match",
    "match-lambda",
    "begin0",
    "case-lambda",
    "parameterize",
    "define",
    "lambda",
    "let",
    "let*",
    "letrec",
    "when",
    "unless",
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("read fixture")
}

/// Counts, per anchored head, how many list nodes carry it — including the ones
/// the rules will correctly decline, since the point is to prove the corpus
/// exercises the head at all.
fn candidate_counts(source: &str) -> BTreeMap<&'static str, usize> {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Racket).expect("parse fixture");
    let mut counts: BTreeMap<&'static str, usize> =
        ANCHOR_HEADS.iter().map(|head| (*head, 0)).collect();

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
    counts
}

#[test]
fn a_realistic_racket_file_earns_no_findings() {
    let source = read("clean.rkt");
    let fired = rules_fired(&source, Dialect::Racket, &fixture("clean.rkt"));
    assert_eq!(fired, Vec::<&str>::new(), "clean.rkt must be clean");
}

/// Without this, the test above would pass over an empty file.
#[test]
fn the_clean_corpus_actually_contains_every_anchored_head() {
    let counts = candidate_counts(&read("clean.rkt"));
    for head in ANCHOR_HEADS {
        assert!(
            counts[head] > 0,
            "clean.rkt contains no `{head}` form, so its zero findings prove nothing"
        );
    }
}

/// The clean file exercises the near-miss shapes heavily, not just once each.
/// A corpus with a single `match` in it would be a much weaker control than the
/// count assertion above suggests.
#[test]
fn the_clean_corpus_exercises_the_sharpest_heads_more_than_once() {
    let counts = candidate_counts(&read("clean.rkt"));
    assert!(counts["match"] >= 3, "match: {}", counts["match"]);
    assert!(counts["define"] >= 8, "define: {}", counts["define"]);
}

#[test]
fn the_dangerous_racket_twin_fires_each_rule_exactly_once() {
    let source = read("dangerous.rkt");
    let mut fired = rules_fired(&source, Dialect::Racket, &fixture("dangerous.rkt"));
    fired.sort_unstable();
    assert_eq!(fired, RULE_NAMES.to_vec());
}

/// The dangerous twin holds the same anchored heads as the clean one, so the
/// difference between them is the defect rather than the vocabulary.
#[test]
fn the_dangerous_twin_anchors_on_the_same_heads() {
    let counts = candidate_counts(&read("dangerous.rkt"));
    for head in ["match", "begin0", "case-lambda", "parameterize", "define"] {
        assert!(counts[head] > 0, "dangerous.rkt contains no `{head}`");
    }
}

/// A dialect outside this package's scope sees nothing at all, however
/// Racket-shaped the source looks. The dialect filter runs before the walk, so
/// this also proves the package costs a Common Lisp file nothing.
///
/// Scheme is the sharpest control: it shares Racket's whole surface syntax,
/// including the brackets, so it is the one dialect that parses this fixture
/// identically and must still see nothing.
#[test]
fn a_scheme_file_sees_none_of_these_rules() {
    let source = read("dangerous.rkt");
    let fired = rules_fired(&source, Dialect::Scheme, Path::new("dangerous.scm"));
    assert_eq!(fired, Vec::<&str>::new());
}
