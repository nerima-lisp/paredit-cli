//! Common Lisp redundant-`parse-integer`-`:radix`-`10` detection: a call
//! `(parse-integer s :radix 10)`. CLHS defines `parse-integer` as
//! `parse-integer string &key start end radix junk-allowed` with `radix`
//! defaulting to `10`, so `(parse-integer s :radix 10)` restates the default and
//! is exactly `(parse-integer s)`.
//!
//! Only a bare integer `10` literal value (no reader prefixes) is flagged; a
//! non-`10` radix (`(parse-integer s :radix 16)`) is meaningful and left alone.
//! Other keyword arguments (`:start`, `:junk-allowed`, …) are preserved.
//!
//! The fix deletes the redundant ` :radix 10` argument pair (from the end of the
//! preceding argument through the `10`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare `:radix` keyword atom.
fn is_radix_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":radix"))
}

/// Whether `view` is the bare integer `10` literal (no reader prefixes).
fn is_ten_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view).is_some_and(|t| t == "10")
}

#[derive(Debug, Clone)]
pub struct ParseIntegerDefaultRadixItem {
    pub path: PathBuf,
    /// The span of the whole `(parse-integer …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :radix 10` argument pair.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct ParseIntegerDefaultRadixSummary {
    pub call_form_count: usize,
    pub violations: Vec<ParseIntegerDefaultRadixItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ParseIntegerDefaultRadixPolicyOptions {
    fail_on_violation: bool,
}

impl ParseIntegerDefaultRadixPolicyOptions {
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
pub struct ParseIntegerDefaultRadixPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Whether a `:radix` argument is provably ten.
///
/// The standalone `inspect parse-integer-default-radix` command reads only the
/// literal `10`, having no semantic tables to consult. The lint suite passes a
/// test that resolves constants and folds arithmetic, so it also sees `#xA`
/// and `(let ((r 10)) … :radix r)` — the same redundant argument, spelled
/// differently.
pub type IsDefaultRadix<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    is_ten: IsDefaultRadix<'_>,
    call_form_count: &mut usize,
    violations: &mut Vec<ParseIntegerDefaultRadixItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("parse-integer") {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_radix_keyword(&view.children[index]) {
            continue;
        }
        if !is_ten(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(ParseIntegerDefaultRadixItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
        });
        return;
    }
}

/// Collects every `(parse-integer s :radix 10)` across a whole file, along with
/// the total number of `parse-integer` calls scanned.
pub fn collect_parse_integer_default_radixes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<ParseIntegerDefaultRadixItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                path,
                &is_ten_literal,
                &mut call_form_count,
                &mut violations,
            );
        });
    }
    Ok((call_form_count, violations))
}

#[must_use]
pub const fn summarize_parse_integer_default_radixes(
    call_form_count: usize,
    violations: Vec<ParseIntegerDefaultRadixItem>,
) -> ParseIntegerDefaultRadixSummary {
    ParseIntegerDefaultRadixSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_parse_integer_default_radix_policy(
    options: ParseIntegerDefaultRadixPolicyOptions,
    summary: &ParseIntegerDefaultRadixSummary,
) -> ParseIntegerDefaultRadixPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ParseIntegerDefaultRadixPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_form_count: summary.call_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<ParseIntegerDefaultRadixItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_parse_integer_default_radixes(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect parse-integer default radixes")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_radix_ten() {
        let source = "(parse-integer s :radix 10)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :radix 10");
    }

    #[test]
    fn keeps_other_keywords() {
        let source = "(parse-integer s :radix 10 :junk-allowed t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :radix 10");
    }

    #[test]
    fn does_not_flag_non_ten_radix() {
        let (count, violations) = calls("(parse-integer s :radix 16)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_parse_integer() {
        let (_, violations) = calls("(parse-integer s)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(PARSE-INTEGER s :RADIX 10)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(parse-integer s :radix 10)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) = collect_parse_integer_default_radixes(
            &PathBuf::from("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("collect parse-integer default radixes");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(parse-integer s :radix 10)");
        let summary = summarize_parse_integer_default_radixes(count, items);

        let quiet = evaluate_parse_integer_default_radix_policy(
            ParseIntegerDefaultRadixPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_parse_integer_default_radix_policy(
            ParseIntegerDefaultRadixPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
