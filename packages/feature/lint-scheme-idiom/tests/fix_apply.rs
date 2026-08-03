//! Every fix this package offers, applied the way `inspect lint --fix` applies
//! it, and checked against the *source text* rather than against a reparse.
//!
//! The reparse guard the fix command runs before writing
//! (`src/presentation/cli/lint_report/workflow.rs`) cannot catch the failure
//! these tests are about. Deleting a comment leaves the surrounding form
//! perfectly balanced, so corrupted output still parses and the guard passes;
//! the only way to see the loss is to compare the bytes. That is what happens
//! here, through the real dispatcher and the real applier
//! ([`apply_byte_span_edits`]) rather than a local reimplementation of either.

// The shared catalogue is compiled into each test binary separately, and this
// one drives it through `run` alone; `rules_fired` is `corpus`'s entry point.
#[allow(dead_code)]
mod support;

use std::path::Path;

use paredit_core_cli::shared::apply_byte_span_edits;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use support::run;

/// Applies every fix the package offers for `source`, exactly as the fixpoint
/// loop does, and returns the rewritten text. `None` when no fix was offered.
fn apply_all(source: &str, dialect: Dialect) -> Option<String> {
    let edits: Vec<_> = run(source, dialect, Path::new("input.scm"))
        .into_iter()
        .filter_map(|outcome| outcome.into_parts().1)
        .flat_map(|fix| {
            fix.replacements()
                .map(|replacement| (replacement.span(), replacement.text().to_owned()))
                .collect::<Vec<_>>()
        })
        .collect();
    if edits.is_empty() {
        return None;
    }
    let rewritten = apply_byte_span_edits(source, edits).expect("apply edits");
    // The shipped write guard, which must pass — that is the whole point.
    SyntaxTree::parse_with_dialect(&rewritten, dialect).expect("rewritten source must reparse");
    Some(rewritten)
}

/// The ordinary named-let fix, which must still work.
#[test]
fn the_named_let_fix_removes_only_the_loop_name() {
    assert_eq!(
        apply_all("(let loop ((i 0)) (display i))", Dialect::Scheme).as_deref(),
        Some("(let ((i 0)) (display i))")
    );
}

/// A block comment between `let` and the loop name. The deletion span a naive
/// fix computes runs from the end of `let` to the end of the name, which spans
/// the comment; applying it destroys the comment and *still reparses*, so no
/// downstream guard fires.
#[test]
fn a_block_comment_between_the_head_and_the_name_is_never_deleted() {
    let source = "(let #|why|# loop ((i 0)) (display i))";
    assert_eq!(
        apply_all(source, Dialect::Scheme),
        None,
        "no fix may be offered when the gap holds a comment"
    );
}

#[test]
fn a_line_comment_between_the_head_and_the_name_is_never_deleted() {
    let source = "(let ; why\n  loop ((i 0)) (display i))";
    assert_eq!(apply_all(source, Dialect::Scheme), None);
}

/// A `#;` datum comment is the same hazard in a different spelling, and the one
/// most likely to be read as whitespace by a naive blank test.
#[test]
fn a_datum_comment_between_the_head_and_the_name_is_never_deleted() {
    let source = "(let #;(ignored) loop ((i 0)) (display i))";
    assert_eq!(apply_all(source, Dialect::Scheme), None);
}

/// The finding is still reported in every comment case — withholding the fix
/// must not also withhold the diagnosis, or the rule would go silent on
/// exactly the files a human most needs to look at.
#[test]
fn the_finding_survives_even_where_the_fix_is_withheld() {
    for source in [
        "(let #|why|# loop ((i 0)) (display i))",
        "(let ; why\n  loop ((i 0)) (display i))",
        "(let #;(ignored) loop ((i 0)) (display i))",
    ] {
        let outcomes = run(source, Dialect::Scheme, Path::new("input.scm"));
        assert_eq!(outcomes.len(), 1, "{source} must still be reported");
        let (finding, fix) = outcomes
            .into_iter()
            .next()
            .expect("one finding")
            .into_parts();
        assert_eq!(finding.rule, "scheme-named-let-never-recurs");
        assert!(fix.is_none(), "{source} must not carry a fix");
    }
}

/// Whitespace-only gaps, including a multi-line one, still get their fix.
/// Without this the assertions above would pass for a rule that had simply
/// stopped offering fixes at all.
#[test]
fn a_whitespace_only_gap_still_gets_its_fix() {
    assert_eq!(
        apply_all(
            "(let\n    loop\n    ((i 0))\n  (display i))",
            Dialect::Scheme
        )
        .as_deref(),
        Some("(let\n    ((i 0))\n  (display i))")
    );
}

/// The memq/assq fix rewrites the operator and nothing else.
#[test]
fn the_memq_fix_rewrites_only_the_operator() {
    assert_eq!(
        apply_all("(memq 101 codes)", Dialect::Scheme).as_deref(),
        Some("(memv 101 codes)")
    );
    assert_eq!(
        apply_all("(assq 42 table)", Dialect::Scheme).as_deref(),
        Some("(assv 42 table)")
    );
}

/// A comment inside the form the memq fix rewrites is untouched, because that
/// fix replaces the head symbol's own span and never spans a gap.
#[test]
fn the_memq_fix_preserves_surrounding_comments() {
    assert_eq!(
        apply_all("(memq #|key|# 101 codes)", Dialect::Scheme).as_deref(),
        Some("(memv #|key|# 101 codes)")
    );
}
