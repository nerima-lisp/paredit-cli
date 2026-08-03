//! Corpus sweep: realistic correct Scheme and Racket earn nothing, and their
//! dangerous twins earn one finding per rule.
//!
//! The clean half alone would pass for the wrong reason if the rules simply
//! never matched anything, so it also asserts that the corpus really does hold
//! the shapes the rules anchor on — a non-zero *candidate* count per head,
//! counted independently of whether any rule fired. The dangerous half then
//! proves each rule can still fire on the same shapes.

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};

use support::rules_fired;

/// Every rule name this package publishes.
const RULE_NAMES: [&str; 4] = [
    "scheme-begin-single-form",
    "scheme-let-star-independent-bindings",
    "scheme-memq-assq-literal-key",
    "scheme-named-let-never-recurs",
];

/// The rules Racket is in scope for. `scheme-memq-assq-literal-key` is Scheme
/// only: Racket specifies the fixnum and character cases R7RS 6.4 leaves open,
/// so every finding it could produce there would be a false positive.
const RACKET_RULE_NAMES: [&str; 3] = [
    "scheme-begin-single-form",
    "scheme-let-star-independent-bindings",
    "scheme-named-let-never-recurs",
];

/// The heads the rules anchor on. A corpus that contains none of these proves
/// nothing by producing no findings.
const ANCHOR_HEADS: [&str; 5] = ["begin", "let", "let*", "memq", "assq"];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("read fixture")
}

/// Counts, per anchored head, how many list nodes in `source` carry it —
/// including the ones the rules will correctly decline, since the point is to
/// prove the corpus exercises the head at all.
fn candidate_counts(source: &str, dialect: Dialect) -> BTreeMap<&'static str, usize> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse fixture");
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
fn a_realistic_scheme_file_earns_no_findings() {
    let source = read("clean.scm");
    let fired = rules_fired(&source, Dialect::Scheme, &fixture("clean.scm"));
    assert_eq!(fired, Vec::<&str>::new(), "clean.scm must be clean");
}

#[test]
fn a_realistic_racket_file_earns_no_findings() {
    let source = read("clean.rkt");
    let fired = rules_fired(&source, Dialect::Racket, &fixture("clean.rkt"));
    assert_eq!(fired, Vec::<&str>::new(), "clean.rkt must be clean");
}

/// Without this, both tests above would pass over an empty file.
#[test]
fn the_clean_corpus_actually_contains_every_anchored_head() {
    for (name, dialect) in [
        ("clean.scm", Dialect::Scheme),
        ("clean.rkt", Dialect::Racket),
    ] {
        let counts = candidate_counts(&read(name), dialect);
        for head in ANCHOR_HEADS {
            assert!(
                counts[head] > 0,
                "{name} contains no `{head}` form, so its zero findings prove nothing"
            );
        }
    }
}

#[test]
fn the_dangerous_scheme_twin_fires_each_rule_exactly_once() {
    let source = read("dangerous.scm");
    let mut fired = rules_fired(&source, Dialect::Scheme, &fixture("dangerous.scm"));
    fired.sort_unstable();
    assert_eq!(fired, RULE_NAMES.to_vec());
}

#[test]
fn the_dangerous_racket_twin_fires_each_racket_scoped_rule_exactly_once() {
    let source = read("dangerous.rkt");
    let mut fired = rules_fired(&source, Dialect::Racket, &fixture("dangerous.rkt"));
    fired.sort_unstable();
    assert_eq!(fired, RACKET_RULE_NAMES.to_vec());
}

/// The Scheme-only rule really is Scheme-only: the shape it reports, handed to
/// it as Racket, earns nothing. Without this the Racket expectation above
/// would pass even if the rule had simply stopped working.
#[test]
fn the_scheme_only_rule_declines_racket_on_a_shape_it_would_report() {
    let source = "(define (known-code? codes)\n  (memq 101 codes))\n";
    assert_eq!(
        rules_fired(source, Dialect::Scheme, Path::new("a.scm")),
        vec!["scheme-memq-assq-literal-key"]
    );
    assert_eq!(
        rules_fired(source, Dialect::Racket, Path::new("a.rkt")),
        Vec::<&str>::new()
    );
}

/// A dialect outside this package's scope must see nothing at all, however
/// Scheme-shaped the source looks. The dialect filter runs before the walk, so
/// this also proves the package costs a Common Lisp file nothing.
#[test]
fn a_common_lisp_file_sees_none_of_these_rules() {
    let source = read("dangerous.scm");
    let fired = rules_fired(&source, Dialect::CommonLisp, Path::new("dangerous.lisp"));
    assert_eq!(fired, Vec::<&str>::new());
}
