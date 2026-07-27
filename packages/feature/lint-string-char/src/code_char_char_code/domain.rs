//! Common Lisp `code-char`-of-`char-code` detection: a `(code-char (char-code c))`.
//!
//! `char-code` maps a character to its code and `code-char` maps that code back to
//! the same character, so `(code-char (char-code c))` is exactly `c` — same
//! character, `c` evaluated once. The bare `c` reads more directly than the
//! round-trip pair.
//!
//! Only the exact `(code-char (char-code c))` two-level shape is matched:
//! `code-char` with one argument, whose argument is `(char-code c)` with exactly
//! one operand. The reverse direction `(char-code (code-char n))` is NOT handled:
//! `code-char` can return `nil` for codes with no corresponding character, so the
//! reverse is not an identity and cannot be safely unwrapped. A reader-conditional
//! operand is left alone.
//!
//! The fix rewrites `(code-char (char-code c))` as `c`, copying the character
//! operand verbatim, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CodeCharCharCodeItem {
    pub path: PathBuf,
    /// The span of the whole `(code-char (char-code c))` form.
    pub span: ByteSpan,
    /// The span of the character operand `c`.
    pub char_span: ByteSpan,
}

#[derive(Debug)]
pub struct CodeCharCharCodeSummary {
    pub code_char_form_count: usize,
    pub violations: Vec<CodeCharCharCodeItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CodeCharCharCodePolicyOptions {
    fail_on_violation: bool,
}

impl CodeCharCharCodePolicyOptions {
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
pub struct CodeCharCharCodePolicy {
    pub fail_on_violation: bool,
    pub code_char_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    code_char_form_count: &mut usize,
    violations: &mut Vec<CodeCharCharCodeItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("code-char") {
        return;
    }
    *code_char_form_count += 1;

    // children: [code-char, inner] — code-char takes exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    if !inner_head.eq_ignore_ascii_case("char-code") {
        return;
    }
    // inner children: [char-code, c].
    if inner.children.len() != 2 {
        return;
    }
    let character = &inner.children[1];
    if is_reader_conditional(character) {
        return;
    }

    violations.push(CodeCharCharCodeItem {
        path: path.to_path_buf(),
        span: view.span,
        char_span: character.span,
    });
}

/// Collects every `(code-char (char-code c))` across a whole file, along with the
/// total number of `code-char` forms scanned.
pub fn collect_code_char_char_codes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CodeCharCharCodeItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut code_char_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut code_char_form_count, &mut violations);
        });
    }
    Ok((code_char_form_count, violations))
}

#[must_use]
pub const fn summarize_code_char_char_codes(
    code_char_form_count: usize,
    violations: Vec<CodeCharCharCodeItem>,
) -> CodeCharCharCodeSummary {
    CodeCharCharCodeSummary {
        code_char_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_code_char_char_code_policy(
    options: CodeCharCharCodePolicyOptions,
    summary: &CodeCharCharCodeSummary,
) -> CodeCharCharCodePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CodeCharCharCodePolicy {
        fail_on_violation: options.fail_on_violation(),
        code_char_form_count: summary.code_char_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_chars(input: &str) -> (usize, Vec<CodeCharCharCodeItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_code_char_char_codes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect code char char codes")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_code_char_char_code() {
        let source = "(code-char (char-code c))";
        let (count, violations) = code_chars(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].char_span), "c");
    }

    #[test]
    fn preserves_compound_char() {
        let source = "(code-char (char-code (elt s i)))";
        let (_, violations) = code_chars(source);
        assert_eq!(slice(source, violations[0].char_span), "(elt s i)");
    }

    #[test]
    fn does_not_flag_plain_char_code() {
        let (_, violations) = code_chars("(char-code c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_reverse() {
        let (_, violations) = code_chars("(char-code (code-char n))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = code_chars("(CODE-CHAR (CHAR-CODE c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = code_chars("(defun f (c) (code-char (char-code c)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(code-char (char-code c))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_code_char_char_codes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect code char char codes");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = code_chars("(code-char (char-code c))");
        let summary = summarize_code_char_char_codes(count, items);

        let quiet = evaluate_code_char_char_code_policy(
            CodeCharCharCodePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_code_char_char_code_policy(CodeCharCharCodePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
