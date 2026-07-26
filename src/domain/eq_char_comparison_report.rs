//! Common Lisp `eq`-on-a-character detection: a call to `eq` with a character
//! literal argument, such as `(eq c #\a)` or `(eq (char s 0) #\Space)`. `eq`
//! tests object identity, and the standard does *not* require two characters to
//! be `eq` even when they denote the same character — only `eql` (and `char=`)
//! are guaranteed. Many implementations happen to intern base characters so
//! `(eq c #\a)` appears to work, but this is not portable and breaks for
//! extended characters. The correct comparison is `eql` or `char=`.
//!
//! Only `eq` is covered: `eql` and `char=` compare characters correctly and are
//! never flagged. A character literal is recognized by the `#\` reader syntax,
//! which covers both single characters (`#\a`) and named characters
//! (`#\Newline`, `#\Space`).
//!
//! This is the character sibling of `eq-number-comparison` — the same identity
//! pitfall, a different literal type — including in how the bug is detected:
//! the CLHS groups characters with numbers as objects an implementation may
//! copy at any time, so `eq` is unreliable on any character whatever produced
//! it. Callers that have a type context therefore pass a second test — see
//! [`IsCharacterArgument`] — which catches `(eq (char s 0) c)` too.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

fn character_argument(view: &ExpressionView) -> Option<&str> {
    atom_text(view).filter(|text| text.starts_with("#\\"))
}

/// Why an argument counts as a character.
///
/// An enum rather than an empty `literal`, because "recognized without a
/// spelling to quote" and "recognized by the empty spelling" are different
/// facts and only one of them is ever true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterEvidence {
    /// A character literal written at the call site: `(eq c #\a)`.
    Literal(String),
    /// An argument a type context proves is a character however it is
    /// spelled: `(eq (char s 0) c)`.
    InferredType,
}

#[derive(Debug, Clone)]
pub struct EqCharComparisonItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The span of the `eq` head symbol, for an `eq` -> `eql` fix.
    pub head_span: ByteSpan,
    pub evidence: CharacterEvidence,
}

impl EqCharComparisonItem {
    /// The literal spelling this was recognized by.
    ///
    /// Empty for a type-derived detection, which the standalone `inspect`
    /// command never produces — it passes [`never`], so every item it renders
    /// carries a spelling.
    #[must_use]
    pub fn literal(&self) -> &str {
        match &self.evidence {
            CharacterEvidence::Literal(text) => text,
            CharacterEvidence::InferredType => "",
        }
    }
}

#[derive(Debug)]
pub struct EqCharComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<EqCharComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EqCharComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl EqCharComparisonPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EqCharComparisonPolicy {
    pub fail_on_violation: bool,
    pub comparison_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Whether an argument is provably a character without being spelled as one.
///
/// The standalone `inspect eq-char-comparison` command has no semantic tables
/// to consult, so it passes [`never`] and keeps reading literals only. The
/// lint suite passes a test backed by the type context, so it also sees
/// `(eq (char s 0) c)` — the same unreliable comparison, spelled in a way the
/// reader alone cannot recognize.
pub(crate) type IsCharacterArgument<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// The [`IsCharacterArgument`] of a caller with no type context.
const fn never(_: &ExpressionView) -> bool {
    false
}

pub(crate) fn examine_comparison(
    view: &ExpressionView,
    path: &Path,
    is_character: IsCharacterArgument<'_>,
    comparison_form_count: &mut usize,
    violations: &mut Vec<EqCharComparisonItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("eq")) {
        return;
    }
    *comparison_form_count += 1;

    // Report the first character argument (after the operator); a call with
    // two characters is still one bug, not two. A literal is looked for across
    // every argument before the type context is asked about any, so a call
    // that has one is still reported by its spelling.
    let arguments = || view.children.iter().skip(1);
    let evidence = arguments()
        .find_map(character_argument)
        .map(|literal| CharacterEvidence::Literal(literal.to_owned()))
        .or_else(|| {
            arguments()
                .any(is_character)
                .then_some(CharacterEvidence::InferredType)
        });

    if let Some(evidence) = evidence {
        violations.push(EqCharComparisonItem {
            path: path.to_path_buf(),
            span: view.span,
            head_span: view.children[0].span,
            evidence,
        });
    }
}

/// Collects every `eq` call with a character-literal argument across a whole
/// file, along with the total number of `eq` calls scanned.
pub fn collect_eq_char_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EqCharComparisonItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(
                subview,
                path,
                &never,
                &mut comparison_form_count,
                &mut violations,
            );
        });
    }
    Ok((comparison_form_count, violations))
}

#[must_use]
pub const fn summarize_eq_char_comparisons(
    comparison_form_count: usize,
    violations: Vec<EqCharComparisonItem>,
) -> EqCharComparisonSummary {
    EqCharComparisonSummary {
        comparison_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_eq_char_comparison_policy(
    options: EqCharComparisonPolicyOptions,
    summary: &EqCharComparisonSummary,
) -> EqCharComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EqCharComparisonPolicy {
        fail_on_violation: options.fail_on_violation(),
        comparison_form_count: summary.comparison_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparisons(input: &str) -> (usize, Vec<EqCharComparisonItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_eq_char_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect eq char comparisons")
    }

    #[test]
    fn flags_eq_against_a_character_literal() {
        let (count, violations) = comparisons("(eq c #\\a)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal(), "#\\a");
    }

    #[test]
    fn flags_eq_against_a_named_character() {
        let (_, violations) = comparisons("(eq (char s 0) #\\Space)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal(), "#\\Space");
    }

    #[test]
    fn does_not_flag_eql_or_char_equal() {
        let (count, violations) = comparisons("(and (eql c #\\a) (char= c #\\a))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eq_between_two_symbols() {
        let (count, violations) = comparisons("(eq x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eq_against_a_number() {
        // That is the eq-number-comparison rule's territory, not this one.
        let (_, violations) = comparisons("(eq n 5)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (count, violations) = comparisons("(defun f (c) (when (eq c #\\z) :zed))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn the_standalone_collector_only_ever_reports_a_spelling() {
        // It passes `never`, so the type-derived case cannot arise here and
        // the rendered `literal=` field is always populated.
        assert!(comparisons("(eq (char s 0) c)").1.is_empty());
        let (_, violations) = comparisons("(eq c #\\a)");
        assert_eq!(
            violations[0].evidence,
            CharacterEvidence::Literal("#\\a".to_owned())
        );
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        // Parse as Common Lisp (whose `#\a` char syntax the tree understands),
        // but collect as Clojure to prove the dialect gate short-circuits.
        let tree =
            SyntaxTree::parse_with_dialect("(eq c #\\a)", Dialect::CommonLisp).expect("parse");
        let (count, violations) =
            collect_eq_char_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect eq char comparisons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = comparisons("(eq c #\\a)");
        let summary = summarize_eq_char_comparisons(count, items);

        let quiet =
            evaluate_eq_char_comparison_policy(EqCharComparisonPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_eq_char_comparison_policy(EqCharComparisonPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
