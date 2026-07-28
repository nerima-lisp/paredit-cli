//! `copy-before-destructive`: copying a sequence so a destructive operator can
//! safely destroy the copy.
//!
//! `(nreverse (copy-list xs))` allocates a list, reverses it in place, and
//! returns it. `(reverse xs)` allocates a list and returns it reversed. Same
//! result, same allocation count on paper — but the copy-then-`nreverse` pair
//! walks the sequence twice and says, in code, something it does not mean.
//!
//! Every pair here has a non-destructive operator that *is* the pair. That is
//! what makes the fix safe and what makes the rule worth having: the rewrite is
//! not "remove the copy" (which would be a bug — `nreverse` would then destroy
//! the caller's list) but "collapse both into the operator that already does
//! this".
//!
//! Fixable, and tagged `destructive` — not because the result differs, but
//! because the fix substitutes an operator rather than deleting a wrapper, and
//! a reader auditing an automated rewrite should see that distinction.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleExplanation, RuleFix,
    RuleMeta, RuleTag, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::shared::{symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "copy-before-destructive",
    RuleCategory::Allocation,
    Severity::Warning,
    "a fresh copy fed to a destructive operator, where one non-destructive operator does both",
    Fixability::Fixable,
)
.with_tags(&[RuleTag::Destructive])
.with_explanation(
    RuleExplanation::new(
        "Copying a sequence so that a destructive operator may safely destroy the copy is exactly \
         what the operator's non-destructive twin does, in one pass instead of two.",
    )
    .with_example("(nreverse (copy-list xs))", "(reverse xs)")
    .with_caveat(
        "Only the copy in the *destroyed* position is read. `(nconc as (copy-list bs))` keeps its \
         copy — `nconc` does not modify its last argument, so the copy there is protecting \
         nothing this rule can prove, and is `unnecessary-copy`'s business if anything.",
    ),
);

/// The copy constructors that make a destructive call safe.
const COPIES: [&str; 4] = ["copy-seq", "copy-list", "copy-alist", "copy-tree"];

/// `(destructive, non-destructive, the argument position it destroys)`.
///
/// The position is the argument the operator is permitted to modify — the only
/// one whose copy this rule may collapse. `nconc` destroys all but its last
/// argument, and this reads the first, which is the spelling that occurs.
const PAIRS: [(&str, &str, usize); 7] = [
    ("nreverse", "reverse", 1),
    ("nconc", "append", 1),
    ("nsubstitute", "substitute", 3),
    ("nunion", "union", 1),
    ("nintersection", "intersection", 1),
    ("nset-difference", "set-difference", 1),
    ("nsubst", "subst", 3),
];

/// One collapsible copy-then-destroy pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyThenDestroy {
    /// The whole destructive call.
    pub span: ByteSpan,
    /// The span of the destructive operator symbol, replaced by its twin.
    pub operator_span: ByteSpan,
    /// The `(copy-* x)` form, replaced by `x`.
    pub copy_span: ByteSpan,
    /// The `x` inside it.
    pub source_span: ByteSpan,
    pub destructive: String,
    pub replacement: &'static str,
}

/// Reads one destructive call and reports the copy it wraps, if any.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<CopyThenDestroy> {
    let head = list_head(view)?;
    let (destructive, replacement, position) = PAIRS
        .iter()
        .find(|(name, _, _)| symbol_in(head, &[name]))
        .copied()?;

    let argument = view.children.get(position)?;
    if !argument.reader_prefixes.is_empty() {
        return None;
    }
    let copy_head = list_head(argument)?;
    if !symbol_in(copy_head, &COPIES) || argument.children.len() != 2 {
        return None;
    }

    Some(CopyThenDestroy {
        span: view.span,
        operator_span: view.children[0].span,
        copy_span: argument.span,
        source_span: argument.children[1].span,
        destructive: unqualified(destructive).to_ascii_lowercase(),
        replacement,
    })
}

const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("nreverse"),
    NormalizedHead::new("nconc"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nunion"),
    NormalizedHead::new("nintersection"),
    NormalizedHead::new("nset-difference"),
    NormalizedHead::new("nsubst"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        // Two disjoint edits, applied together: substitute the operator, and
        // unwrap the copy. Doing either alone would be a bug — dropping the
        // copy while `nreverse` remains would destroy the caller's sequence.
        let fix = RuleFix::multi(
            format!(
                "Replace {} of a copy with {}",
                found.destructive, found.replacement
            ),
            Replacement::new(found.operator_span, found.replacement),
            [Replacement::new(
                found.copy_span,
                context.slice(found.source_span).to_owned(),
            )],
        );
        sink.report_fixed(
            found.span,
            format!(
                "{} destroys a copy made for it; {} does both in one pass",
                found.destructive, found.replacement
            ),
            fix,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn view(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn replacement(input: &str) -> Option<&'static str> {
        examine(&view(input)).map(|found| found.replacement)
    }

    #[test]
    fn flags_the_canonical_nreverse_of_a_copy() {
        assert_eq!(replacement("(nreverse (copy-list xs))"), Some("reverse"));
        assert_eq!(replacement("(nreverse (copy-seq xs))"), Some("reverse"));
    }

    #[test]
    fn flags_every_pair() {
        assert_eq!(replacement("(nconc (copy-list as) bs)"), Some("append"));
        assert_eq!(replacement("(nunion (copy-list as) bs)"), Some("union"));
        assert_eq!(
            replacement("(nsubst new old (copy-tree form))"),
            Some("subst")
        );
    }

    #[test]
    fn reads_only_the_position_the_operator_destroys() {
        // `nconc` does not modify its last argument, so a copy there is not
        // this rule's finding.
        assert_eq!(replacement("(nconc as (copy-list bs))"), None);
    }

    #[test]
    fn does_not_flag_a_destructive_call_on_a_bare_sequence() {
        // This one is correct and load-bearing: the caller meant to destroy it.
        assert_eq!(replacement("(nreverse xs)"), None);
    }

    #[test]
    fn does_not_flag_a_non_copy_argument() {
        assert_eq!(replacement("(nreverse (remove nil xs))"), None);
    }

    #[test]
    fn does_not_flag_a_multi_argument_copy_call() {
        assert_eq!(replacement("(nreverse (copy-seq xs 1))"), None);
    }

    #[test]
    fn the_fix_edits_the_operator_and_the_copy_and_nothing_else() {
        let source = "(nreverse (copy-list xs))";
        let found = examine(&view(source)).expect("finding");
        assert_eq!(
            &source[found.operator_span.start().get()..found.operator_span.end().get()],
            "nreverse"
        );
        assert_eq!(
            &source[found.copy_span.start().get()..found.copy_span.end().get()],
            "(copy-list xs)"
        );
        assert_eq!(
            &source[found.source_span.start().get()..found.source_span.end().get()],
            "xs"
        );
    }

    #[test]
    fn the_package_qualified_spelling_is_read_the_same() {
        assert_eq!(
            replacement("(cl:nreverse (cl:copy-list xs))"),
            Some("reverse")
        );
    }
}
