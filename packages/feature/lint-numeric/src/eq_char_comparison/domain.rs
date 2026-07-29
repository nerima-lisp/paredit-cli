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
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

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
    pub span: ByteSpan,
    /// The 1-based line the comparison starts on.
    pub line: usize,
    /// The span of the `eq` head symbol, for an `eq` -> `eql` fix.
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to swap
    /// `eq` for `eql`, and the command never prints it.
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

impl Finding for EqCharComparisonItem {
    /// The rule's own name rather than a variant of it.
    ///
    /// [`CharacterEvidence`] is the only thing that varies, and the standalone
    /// command that produces this report passes [`never`] — so every finding it
    /// can emit is a `Literal` one. A `kind` with a single reachable value
    /// would name a distinction the report cannot make.
    fn kind(&self) -> &'static str {
        "eq-char-comparison"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("literal={}", self.literal())]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("literal", json!(self.literal()))]
    }

    /// The same sentence the `eq-char-comparison` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        match &self.evidence {
            CharacterEvidence::Literal(literal) => {
                format!("eq compares against character literal {literal}; use eql or char=")
            }
            CharacterEvidence::InferredType => {
                "eq compares against an argument of inferred type character; use eql or char="
                    .to_owned()
            }
        }
    }
}

/// Whether an argument is provably a character without being spelled as one.
///
/// The standalone `inspect eq-char-comparison` command has no semantic tables
/// to consult, so it passes [`never`] and keeps reading literals only. The
/// lint suite passes a test backed by the type context, so it also sees
/// `(eq (char s 0) c)` — the same unreliable comparison, spelled in a way the
/// reader alone cannot recognize.
pub type IsCharacterArgument<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// The [`IsCharacterArgument`] of a caller with no type context.
const fn never(_: &ExpressionView) -> bool {
    false
}

pub fn examine_comparison(
    view: &ExpressionView,
    source: &str,
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
            span: view.span,
            line: line_of(source, view.span.start().get()),
            head_span: view.children[0].span,
            evidence,
        });
    }
}

/// Collects every `eq` call with a character-literal argument in one file, with
/// the number of `eq` calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no such comparison here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_eq_char_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EqCharComparisonItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("comparison_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(
                subview,
                source,
                &never,
                &mut comparison_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("comparison_form_count", json!(comparison_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EqCharComparisonItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_eq_char_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eq char comparison report")
    }

    /// The `(comparison_form_count, violations)` pair the report is built from.
    fn comparisons(input: &str) -> (u64, Vec<EqCharComparisonItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "comparison_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("comparison_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        // Parse as Common Lisp (whose `#\a` char syntax the tree understands),
        // but collect as Clojure to prove the dialect gate short-circuits.
        let tree =
            SyntaxTree::parse_with_dialect("(eq c #\\a)", Dialect::CommonLisp).expect("parse");
        let report = build_eq_char_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build eq char comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eq x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_literal() {
        let report = report("(defun f (c)\n  (eq c #\\a))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "eq-char-comparison");
        assert_eq!(finding.json_fields(), vec![("literal", json!("#\\a"))]);
        assert_eq!(finding.text_columns(), vec!["literal=#\\a".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_eq_scanned_not_only_the_flagged_ones() {
        let report = report("(eq c #\\a)\n(eq x y)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
