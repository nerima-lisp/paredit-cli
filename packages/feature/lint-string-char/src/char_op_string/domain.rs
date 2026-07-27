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

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

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

#[derive(Debug, Clone)]
pub struct CharOpStringItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The character function (`char=`, `char-code`, …).
    pub operator: String,
    pub mismatch: CharacterMismatch,
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

#[derive(Debug)]
pub struct CharOpStringSummary {
    pub char_call_count: usize,
    pub violations: Vec<CharOpStringItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CharOpStringPolicyOptions {
    fail_on_violation: bool,
}

impl CharOpStringPolicyOptions {
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
pub struct CharOpStringPolicy {
    pub fail_on_violation: bool,
    pub char_call_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
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
    path: &Path,
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
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_ascii_lowercase(),
            mismatch,
        });
    }
}

/// Collects every character-function call with a string-literal argument across
/// a whole file, along with the total number of character-function calls
/// scanned.
pub fn collect_char_op_strings(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CharOpStringItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut char_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, path, &never, &mut char_call_count, &mut violations);
        });
    }
    Ok((char_call_count, violations))
}

#[must_use]
pub const fn summarize_char_op_strings(
    char_call_count: usize,
    violations: Vec<CharOpStringItem>,
) -> CharOpStringSummary {
    CharOpStringSummary {
        char_call_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_char_op_string_policy(
    options: CharOpStringPolicyOptions,
    summary: &CharOpStringSummary,
) -> CharOpStringPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CharOpStringPolicy {
        fail_on_violation: options.fail_on_violation(),
        char_call_count: summary.char_call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<CharOpStringItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_char_op_strings(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect char op strings")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(char= \"a\" c)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_char_op_strings(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect char op strings");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(char= \"a\" c)");
        let summary = summarize_char_op_strings(count, items);

        let quiet = evaluate_char_op_string_policy(CharOpStringPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_char_op_string_policy(CharOpStringPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
