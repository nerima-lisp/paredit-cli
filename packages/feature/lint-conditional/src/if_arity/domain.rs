//! Common Lisp `if`-arity detection: an `if` special form with the wrong
//! number of arguments. Common Lisp's `if` is `(if test then [else])` — it
//! takes exactly two or three argument forms. Fewer (`(if test)`) or more
//! (`(if test then else extra)`) is a program error, caught only at
//! macroexpansion or compile time rather than by the reader. The classic bug
//! is treating the `else` position as an implicit `progn`: `(if c a b d)`
//! silently drops or errors instead of running `b` then `d`.
//!
//! Scope: Common Lisp only, and for good reason — Emacs Lisp's `if` *does*
//! take an implicit-progn else (`(if c then else...)`), so `(if a b c d)` is
//! valid there. Restricting to Common Lisp keeps "more than three arguments is
//! malformed" a provable claim.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted/unquoted `if` (data or a
//! template, not a call), or any `if` with a child carrying a reader
//! conditional (`#+`/`#-` expand to zero or one form) or a splicing unquote
//! (`,@` splices an unknown number of forms).
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Whether a child form can change how many argument forms actually reach the
/// evaluator, making a static arity count unreliable. Two cases matter:
///
/// * A `,@` splice in a template (or a Clojure `#?`/`#?@` reader conditional),
///   which the reader models as a prefix on the following form.
/// * A Common Lisp `#+`/`#-` reader conditional, which expands to zero or one
///   form. Depending on the parse mode the reader models it either as a bare
///   `#+`/`#-` marker atom or as a single atom whose text begins `#+`/`#-`
///   (e.g. `#+sbcl c`); both start with the marker, so `starts_with` covers
///   the pair.
fn is_arity_ambiguous_child(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct IfArityItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub argument_count: usize,
}

#[derive(Debug)]
pub struct IfAritySummary {
    pub if_form_count: usize,
    pub violations: Vec<IfArityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct IfArityPolicyOptions {
    fail_on_violation: bool,
}

impl IfArityPolicyOptions {
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
pub struct IfArityPolicy {
    pub fail_on_violation: bool,
    pub if_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    path: &Path,
    if_form_count: &mut usize,
    violations: &mut Vec<IfArityItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("if")) {
        return;
    }
    // A quoted/quasiquoted/unquoted `if` is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A child that expands to zero-or-one form (`#+`/`#-`) or splices an
    // unknown number of forms (`,@`) makes the written arity unreliable.
    if view.children.iter().any(is_arity_ambiguous_child) {
        return;
    }
    *if_form_count += 1;

    // children = [if, test, then, else?]; the head does not count.
    let argument_count = view.children.len() - 1;
    if !(2..=3).contains(&argument_count) {
        violations.push(IfArityItem {
            path: path.to_path_buf(),
            span: view.span,
            argument_count,
        });
    }
}

/// Collects every misarity `if` form across a whole file, along with the total
/// number of `if` forms scanned.
pub fn collect_if_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<IfArityItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, path, &mut if_form_count, &mut violations);
        });
    }
    Ok((if_form_count, violations))
}

#[must_use]
pub const fn summarize_if_arity(
    if_form_count: usize,
    violations: Vec<IfArityItem>,
) -> IfAritySummary {
    IfAritySummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_if_arity_policy(
    options: IfArityPolicyOptions,
    summary: &IfAritySummary,
) -> IfArityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    IfArityPolicy {
        fail_on_violation: options.fail_on_violation(),
        if_form_count: summary.if_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<IfArityItem>) {
        // Use the dialect-aware parse the CLI path uses: it groups Common Lisp
        // `#+`/`#-` reader conditionals differently than the default reader.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_if_arity_violations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect if arity violations")
    }

    #[test]
    fn flags_too_many_arguments() {
        let (if_form_count, items) = violations("(if a b c d)");
        assert_eq!(if_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 4);
    }

    #[test]
    fn flags_too_few_arguments() {
        let (_, items) = violations("(if a)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 1);
    }

    #[test]
    fn does_not_flag_a_two_argument_if() {
        let (if_form_count, items) = violations("(if a b)");
        assert_eq!(if_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_three_argument_if() {
        let (_, items) = violations("(if a b c)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_an_if_with_a_reader_conditional_else() {
        // Only one of the two feature branches survives at read time, so the
        // written four-argument shape is not a real arity error.
        let (if_form_count, items) = violations("(if a b #+sbcl c #-sbcl d)");
        assert_eq!(if_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_if_form() {
        let (if_form_count, items) = violations("(list '(if a b c d))");
        assert_eq!(if_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_an_if_nested_in_a_function_body() {
        let (if_form_count, items) = violations("(defun f (x) (if x 1 2 3))");
        assert_eq!(if_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 4);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        // Emacs Lisp `if` has an implicit-progn else, so this is valid there.
        let tree = SyntaxTree::parse_with_dialect("(if a b c d)", Dialect::EmacsLisp)
            .expect("parse input");
        let (if_form_count, items) =
            collect_if_arity_violations(&PathBuf::from("app.el"), Dialect::EmacsLisp, &tree)
                .expect("collect if arity violations");
        assert_eq!(if_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (if_form_count, items) = violations("(if a b c d)");
        let summary = summarize_if_arity(if_form_count, items);

        let quiet = evaluate_if_arity_policy(IfArityPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_if_arity_policy(IfArityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
