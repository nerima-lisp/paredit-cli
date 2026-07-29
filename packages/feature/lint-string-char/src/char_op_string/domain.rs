//! Common Lisp character-function-on-a-string detection: a function that
//! requires character arguments — `char=`, `char<`, `char-code`, `char-upcase`,
//! `alpha-char-p`, and friends — applied to a *string literal*, such as
//! `(char= "a" c)` or `(char-code "x")`. A one-character string is not a
//! character (`"a"` ≠ `#\a`), and none of these functions accept a string in
//! any argument position, so the call is a guaranteed type error at run time.
//! The mistake is usually a `"…"` where a `#\…` character literal was meant.
//!
//! A string literal is only the usual *spelling* of the mistake, not what
//! makes it one: these functions reject anything that is not a character, so
//! `(char= (length xs) c)` is the same guaranteed type error. Callers that
//! have a type context therefore pass a second test — see
//! [`IsNonCharacterArgument`] — which catches those too.
//!
//! Note the direction. The test asks whether an argument is provably *not* a
//! character, never whether it is a string: an argument the type layer cannot
//! settle answers "no" to both, and only the negative question keeps an
//! unsettled argument silent while still catching an integer or a list.
//!
//! Only a *string literal* argument is flagged by spelling; a character
//! literal, a symbol, or any other form is left alone. This is the
//! character-function sibling of `eql-string-comparison`, which covers the
//! same string/char confusion for `eq`/`eql`.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Functions that require character arguments (a string literal is a type error
/// in any argument position).
const CHAR_FUNCS: [&str; 25] = [
    "char=",
    "char/=",
    "char<",
    "char>",
    "char<=",
    "char>=",
    "char-equal",
    "char-not-equal",
    "char-lessp",
    "char-greaterp",
    "char-not-lessp",
    "char-not-greaterp",
    "char-code",
    "char-int",
    "char-upcase",
    "char-downcase",
    "char-name",
    "digit-char-p",
    "alpha-char-p",
    "alphanumericp",
    "upper-case-p",
    "lower-case-p",
    "both-case-p",
    "graphic-char-p",
    "standard-char-p",
];

/// The first argument (after the operator) that cannot be a character, and
/// why.
///
/// The spelling test runs first so a call written with a string literal keeps
/// naming it, which is what leaves the standalone command's output untouched
/// and its message the more useful of the two.
fn first_non_character_argument<'a>(
    view: &'a ExpressionView,
    is_non_character: IsNonCharacterArgument<'_>,
) -> Option<(&'a ExpressionView, CharacterMismatch)> {
    view.children
        .iter()
        .skip(1)
        .find_map(|child| {
            atom_text(child)
                .filter(|text| text.starts_with('"'))
                .map(|text| (child, CharacterMismatch::StringLiteral(text.to_owned())))
        })
        .or_else(|| {
            view.children
                .iter()
                .skip(1)
                .find(|child| is_non_character(child))
                .map(|child| (child, CharacterMismatch::InferredType))
        })
}

/// Why an argument cannot be a character.
///
/// An enum rather than an empty `literal`, because "recognized without a
/// spelling to quote" and "recognized by the empty spelling" are different
/// facts and only one of them is ever true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterMismatch {
    /// A string literal written at the call site: `(char= "a" c)`.
    StringLiteral(String),
    /// An argument a type context proves is not a character however it is
    /// spelled: `(char= (length xs) c)`.
    InferredType,
}

impl CharacterMismatch {
    /// How the argument was recognized, as a stable token.
    ///
    /// The two are separable without parsing JSON because they answer
    /// different questions: a string literal is a typo the author can see in
    /// the source, an inferred type is a conclusion drawn from elsewhere in
    /// the file, and a consumer filtering on one of them is asking a real
    /// question.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::StringLiteral(_) => "string-literal",
            Self::InferredType => "inferred-type",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharOpStringItem {
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The character function (`char=`, `char-code`, …).
    pub operator: String,
    pub mismatch: CharacterMismatch,
}

impl Finding for CharOpStringItem {
    fn kind(&self) -> &'static str {
        self.mismatch.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone(), format!("literal={}", self.literal())]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("literal", json!(self.literal())),
        ]
    }

    /// The same sentence the `char-op-string` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        match &self.mismatch {
            CharacterMismatch::StringLiteral(literal) => format!(
                "{} is given string literal {literal}; it requires a character (type error)",
                self.operator
            ),
            CharacterMismatch::InferredType => format!(
                "{} is given an argument of an inferred non-character type; it requires a character (type error)",
                self.operator
            ),
        }
    }
}

impl CharOpStringItem {
    /// The string literal this was recognized by.
    ///
    /// Empty for a type-derived detection, which the standalone `inspect`
    /// command never produces — it passes [`never`], so every item it renders
    /// carries a spelling.
    #[must_use]
    pub fn literal(&self) -> &str {
        match &self.mismatch {
            CharacterMismatch::StringLiteral(text) => text,
            CharacterMismatch::InferredType => "",
        }
    }
}

/// Whether an argument is provably not a character without being spelled as a
/// string.
///
/// The standalone `inspect char-op-string` command has no semantic tables to
/// consult, so it passes [`never`] and keeps reading literals only. The lint
/// suite passes a test backed by the type context, so it also sees
/// `(char= (length xs) c)` — the same guaranteed type error, spelled in a way
/// the reader alone cannot recognize.
pub type IsNonCharacterArgument<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// The [`IsNonCharacterArgument`] of a caller with no type context.
const fn never(_: &ExpressionView) -> bool {
    false
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    source: &str,
    is_non_character: IsNonCharacterArgument<'_>,
    char_call_count: &mut usize,
    violations: &mut Vec<CharOpStringItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CHAR_FUNCS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *char_call_count += 1;

    if let Some((_, mismatch)) = first_non_character_argument(view, is_non_character) {
        violations.push(CharOpStringItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            operator: head.to_ascii_lowercase(),
            mismatch,
        });
    }
}

/// Collects every character-function call with a string-literal argument in one
/// file, with the number of character-function calls scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every character function here is given a
/// character" for Common Lisp and "nothing was looked for" for Clojure, and the
/// two read identically without the flag.
pub fn build_char_op_string_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CharOpStringItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("char_call_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut char_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(
                subview,
                source,
                &never,
                &mut char_call_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("char_call_count", json!(char_call_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CharOpStringItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_char_op_string_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build char op string report")
    }

    /// The `(char_call_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<CharOpStringItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "char_call_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("char_call_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_char_equal_with_a_string() {
        let (count, violations) = calls("(char= \"a\" c)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "char=");
        assert_eq!(violations[0].literal(), "\"a\"");
    }

    #[test]
    fn flags_a_string_in_a_later_argument() {
        let (_, violations) = calls("(char< c \"z\")");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_char_code_of_a_string() {
        let (_, violations) = calls("(char-code \"x\")");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "char-code");
    }

    #[test]
    fn flags_predicate_of_a_string() {
        let (_, violations) = calls("(alpha-char-p \"a\")");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_character_literal() {
        let (count, violations) = calls("(char= #\\a c)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_variables() {
        let (_, violations) = calls("(char= a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_string_functions() {
        // string= accepts string designators; a string is fine there.
        let (count, violations) = calls("(string= \"a\" b)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = calls("(CHAR= \"a\" c)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_call_nested_in_a_body() {
        let (_, violations) = calls("(defun f (c) (when (char-equal c \"z\") :zed))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(char= \"a\" c)", Dialect::Clojure).expect("parse");
        let report = build_char_op_string_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build char op string report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("char_call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(char= a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_literal() {
        let report = report("(defun f (c)\n  (char= c \"a\"))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "string-literal");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("char=")), ("literal", json!("\"a\""))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["char=".to_owned(), "literal=\"a\"".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "char= is given string literal \"a\"; it requires a character (type error)"
        );
    }

    #[test]
    fn the_summary_counts_every_character_call_scanned_not_only_the_flagged_ones() {
        let report = report("(char= \"a\" c)\n(char= a b)\n(char-code c)\n");
        assert_eq!(report.summary, vec![("char_call_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
