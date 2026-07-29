//! `contradictory-optimize`: two `optimize` qualities that cannot both hold.
//!
//! `(declare (optimize (speed 3) (speed 0)))` names one quality twice with two
//! values, and which one the compiler takes is not specified. `(optimize (speed
//! 3) (debug 3))` is not a contradiction in the same sense — both are legal —
//! but no implementation can honour both, and every one of them silently
//! prefers one, so the declaration says less than its author thinks.
//!
//! Only the first kind is reported. The second is a real cost but a defensible
//! choice, and a rule that argued with `(speed 3) (debug 3)` would be arguing
//! about compilers rather than about the code.
//!
//! Report-only: which of the two values was meant is exactly the missing
//! information.
//!
//! Scope: Common Lisp only.

use std::collections::BTreeMap;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "contradictory-optimize",
    RuleCategory::Declaration,
    Severity::Error,
    "an optimize declaration naming one quality twice with different values",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "The standard does not say which of two values for the same optimization quality a \
         compiler takes, so a declaration that gives both means something different on every \
         implementation — and usually something different from what its author intended.",
    )
    .with_example(
        "(declare (optimize (speed 3) (safety 1) (speed 0)))",
        "(declare (optimize (speed 3) (safety 1)))",
    )
    .with_caveat(
        "`(optimize (speed 3) (debug 3))` is not reported. No compiler can fully honour both, but \
         both are legal and the trade-off is the author's to make.",
    ),
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("declare"),
    NormalizedHead::new("declaim"),
];

/// One quality given two values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContradictoryQuality {
    pub span: ByteSpan,
    pub quality: String,
    pub first: String,
    pub second: String,
}

/// Every contradiction in one `declare`/`declaim` form.
///
/// A form can carry several `optimize` specifiers, and the standard reads them
/// together, so they are gathered into one map rather than checked one by one.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<ContradictoryQuality> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["declare", "declaim"]) {
        return Vec::new();
    }

    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut found = Vec::new();
    for specifier in view.children.iter().skip(1) {
        if !list_head(specifier).is_some_and(|name| symbol_in(name, &["optimize"])) {
            continue;
        }
        for quality in specifier.children.iter().skip(1) {
            // `(speed 3)` and the bare `speed` (which means `(speed 3)`) are
            // both legal spellings.
            let (name, value) = match atom_text(quality) {
                Some(bare) => (bare.to_owned(), "3".to_owned()),
                None => {
                    let Some(name) = quality.children.first().and_then(atom_text) else {
                        continue;
                    };
                    let Some(value) = quality.children.get(1).and_then(atom_text) else {
                        continue;
                    };
                    (name.to_owned(), value.to_owned())
                }
            };
            let key = unqualified(&name).to_ascii_lowercase();
            match seen.get(&key) {
                Some(previous) if *previous != value => {
                    found.push(ContradictoryQuality {
                        span: quality.span,
                        quality: key.clone(),
                        first: previous.clone(),
                        second: value.clone(),
                    });
                }
                Some(_) => {}
                None => {
                    seen.insert(key, value);
                }
            }
        }
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for quality in examine(view) {
            sink.report(
                quality.span,
                format!(
                    "{} is declared both {} and {}; the standard does not say which a compiler \
                     takes",
                    quality.quality, quality.first, quality.second
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn qualities(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| item.quality)
            .collect()
    }

    #[test]
    fn flags_one_quality_given_two_values() {
        assert_eq!(
            qualities("(declare (optimize (speed 3) (speed 0)))"),
            vec!["speed"]
        );
    }

    #[test]
    fn reads_across_two_optimize_specifiers() {
        assert_eq!(
            qualities("(declare (optimize (speed 3)) (optimize (speed 1)))"),
            vec!["speed"]
        );
    }

    #[test]
    fn reads_the_bare_quality_spelling_as_three() {
        assert_eq!(
            qualities("(declare (optimize speed (speed 0)))"),
            vec!["speed"]
        );
        assert!(qualities("(declare (optimize speed (speed 3)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_repeated_identical_value() {
        assert!(qualities("(declare (optimize (speed 3) (speed 3)))").is_empty());
    }

    #[test]
    fn does_not_flag_two_different_qualities() {
        assert!(qualities("(declare (optimize (speed 3) (safety 1)))").is_empty());
        // Costly, but legal and deliberate.
        assert!(qualities("(declare (optimize (speed 3) (debug 3)))").is_empty());
    }

    #[test]
    fn reads_declaim_the_same_way() {
        assert_eq!(
            qualities("(declaim (optimize (safety 3) (safety 0)))"),
            vec!["safety"]
        );
    }

    #[test]
    fn does_not_flag_a_declaration_without_optimize() {
        assert!(qualities("(declare (type integer x) (ignore y))").is_empty());
    }

    #[test]
    fn reports_each_contradicted_quality_once_per_extra_value() {
        let found = qualities("(declare (optimize (speed 3) (speed 0) (speed 1)))");
        assert_eq!(found, vec!["speed", "speed"]);
    }
}
