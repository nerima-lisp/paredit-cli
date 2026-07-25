//! Common Lisp redundant-`make-array`-default-keyword detection: a `make-array`
//! call with an explicit `:adjustable nil` or `:fill-pointer nil`. CLHS specifies
//! that both keywords *default to* `nil` (an ordinary, non-adjustable array with
//! no fill pointer), so `(make-array n :adjustable nil)` is exactly
//! `(make-array n)`. The explicit `nil` restates the default and adds only noise.
//!
//! Scope is limited to `:adjustable` and `:fill-pointer` — the two `make-array`
//! keywords that both (a) default to `nil` unconditionally and (b) are
//! independent of every other keyword. `:displaced-to` also defaults to `nil`
//! but interacts with `:displaced-index-offset`, and `:initial-element nil` is
//! *not* redundant for `make-array` (an unspecified element type leaves contents
//! undefined), so neither is touched.
//!
//! Only a bare `nil` literal value is flagged. A non-`nil` value
//! (`:adjustable t`) is meaningful and left alone. The first matching redundant
//! keyword pair in the call is reported.
//!
//! The fix deletes the redundant ` :adjustable nil` / ` :fill-pointer nil`
//! argument pair (from the end of the preceding argument through the `nil`),
//! leaving the rest byte-identical, so the rule is auto-fixable.
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

/// The `make-array` keywords that unconditionally default to `nil` and are
/// independent of the other keyword arguments.
const NIL_KEYWORDS: [&str; 2] = [":adjustable", ":fill-pointer"];

/// Whether `view` is one of the redundant-when-nil keyword atoms.
fn is_nil_default_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view)
            .is_some_and(|text| NIL_KEYWORDS.iter().any(|kw| text.eq_ignore_ascii_case(kw)))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct MakeArrayDefaultKeywordItem {
    pub path: PathBuf,
    /// The span of the whole `(make-array …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :adjustable nil` / ` :fill-pointer nil` pair.
    pub removal_span: ByteSpan,
    /// The keyword name, for the finding message.
    pub keyword: String,
}

#[derive(Debug)]
pub struct MakeArrayDefaultKeywordSummary {
    pub call_form_count: usize,
    pub violations: Vec<MakeArrayDefaultKeywordItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MakeArrayDefaultKeywordPolicyOptions {
    fail_on_violation: bool,
}

impl MakeArrayDefaultKeywordPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct MakeArrayDefaultKeywordPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<MakeArrayDefaultKeywordItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("make-array") {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_nil_default_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let keyword = atom_text(&view.children[index])
            .unwrap_or_default()
            .to_owned();
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(MakeArrayDefaultKeywordItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            keyword,
        });
        return;
    }
}

/// Collects every `make-array` call with a redundant `:adjustable nil` /
/// `:fill-pointer nil` across a whole file, along with the total number of
/// `make-array` calls scanned.
pub fn collect_make_array_default_keywords(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MakeArrayDefaultKeywordItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut call_form_count, &mut violations)
        });
    }
    Ok((call_form_count, violations))
}

pub fn summarize_make_array_default_keywords(
    call_form_count: usize,
    violations: Vec<MakeArrayDefaultKeywordItem>,
) -> MakeArrayDefaultKeywordSummary {
    MakeArrayDefaultKeywordSummary {
        call_form_count,
        violations,
    }
}

pub fn evaluate_make_array_default_keyword_policy(
    options: MakeArrayDefaultKeywordPolicyOptions,
    summary: &MakeArrayDefaultKeywordSummary,
) -> MakeArrayDefaultKeywordPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MakeArrayDefaultKeywordPolicy {
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

    fn calls(input: &str) -> (usize, Vec<MakeArrayDefaultKeywordItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_make_array_default_keywords(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect make-array default keywords")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_adjustable_nil() {
        let source = "(make-array n :adjustable nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :adjustable nil"
        );
    }

    #[test]
    fn flags_fill_pointer_nil() {
        let source = "(make-array n :fill-pointer nil)";
        let (_, violations) = calls(source);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :fill-pointer nil"
        );
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(make-array n :adjustable nil :element-type 'bit)";
        let (_, violations) = calls(source);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :adjustable nil"
        );
    }

    #[test]
    fn does_not_flag_non_nil() {
        let (count, violations) = calls("(make-array n :adjustable t)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_initial_element_nil() {
        // :initial-element nil is NOT redundant for make-array.
        assert!(calls("(make-array n :initial-element nil)").1.is_empty());
    }

    #[test]
    fn case_folds_head_and_keyword() {
        let (_, violations) = calls("(MAKE-ARRAY n :ADJUSTABLE nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(make-array n :adjustable nil)", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_make_array_default_keywords(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect make-array default keywords");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(make-array n :adjustable nil)");
        let summary = summarize_make_array_default_keywords(count, items);

        let quiet = evaluate_make_array_default_keyword_policy(
            MakeArrayDefaultKeywordPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_make_array_default_keyword_policy(
            MakeArrayDefaultKeywordPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
