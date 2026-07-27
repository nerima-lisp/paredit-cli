//! Common Lisp `format`-to-string detection: a `(format nil "~A" x)` or
//! `(format nil "~S" x)` whose control string is exactly the single directive
//! `~A`/`~a` or `~S`/`~s`. With a `nil` destination `format` returns the produced
//! string, and a lone `~A` directive prints its argument with `princ` semantics
//! while `~S` uses `prin1` semantics, so:
//!
//!   - `(format nil "~A" x)` is exactly `(princ-to-string x)`, and
//!   - `(format nil "~S" x)` is exactly `(prin1-to-string x)`.
//!
//! Both name the intent directly and avoid re-parsing a control string at run
//! time. The equivalence is exact: `princ-to-string`/`prin1-to-string` are
//! defined to print as if by `princ`/`prin1`, matching `~A`/`~S`'s binding of
//! `*print-escape*`.
//!
//! Only a control string that is *exactly* the one directive is matched — any
//! surrounding text (`"value: ~A"`), extra directives, a non-`nil` destination
//! (`t` or a stream, whose return value is `nil`, not the string), an argument
//! count other than one, and a reader-conditional argument are all left alone.
//!
//! The fix rewrites the call as `(princ-to-string x)`/`(prin1-to-string x)`,
//! copying the argument's source verbatim, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `nil` destination (no reader prefixes).
fn is_nil_destination(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// The string-producing replacement function for a control string that is
/// exactly one `~A`/`~S` directive, or `None` for any other control string.
/// `text` is the atom's source, including its surrounding quotes.
fn directive_replacement(text: &str) -> Option<&'static str> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    match inner {
        "~a" | "~A" => Some("princ-to-string"),
        "~s" | "~S" => Some("prin1-to-string"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct FormatToStringItem {
    pub path: PathBuf,
    /// The span of the whole `(format nil "~A" x)` form.
    pub span: ByteSpan,
    /// The replacement function name (`princ-to-string` or `prin1-to-string`).
    pub replacement: &'static str,
    /// The span of the single format argument (for reconstructing the fix).
    pub argument_span: ByteSpan,
}

#[derive(Debug)]
pub struct FormatToStringSummary {
    pub format_form_count: usize,
    pub violations: Vec<FormatToStringItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct FormatToStringPolicyOptions {
    fail_on_violation: bool,
}

impl FormatToStringPolicyOptions {
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
pub struct FormatToStringPolicy {
    pub fail_on_violation: bool,
    pub format_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    format_form_count: &mut usize,
    violations: &mut Vec<FormatToStringItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("format") {
        return;
    }
    *format_form_count += 1;

    // children: [format, destination, control, argument] — exactly one argument.
    if view.children.len() != 4 {
        return;
    }
    let destination = &view.children[1];
    let control = &view.children[2];
    let argument = &view.children[3];
    if !is_nil_destination(destination) {
        return;
    }
    if is_reader_conditional(argument) {
        return;
    }
    let Some(replacement) = atom_text(control).and_then(directive_replacement) else {
        return;
    };

    violations.push(FormatToStringItem {
        path: path.to_path_buf(),
        span: view.span,
        replacement,
        argument_span: argument.span,
    });
}

/// Collects every `(format nil "~A"/"~S" x)` across a whole file, along with the
/// total number of `format` forms scanned.
pub fn collect_format_to_strings(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<FormatToStringItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut format_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut format_form_count, &mut violations);
        });
    }
    Ok((format_form_count, violations))
}

#[must_use]
pub const fn summarize_format_to_strings(
    format_form_count: usize,
    violations: Vec<FormatToStringItem>,
) -> FormatToStringSummary {
    FormatToStringSummary {
        format_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_format_to_string_policy(
    options: FormatToStringPolicyOptions,
    summary: &FormatToStringSummary,
) -> FormatToStringPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    FormatToStringPolicy {
        fail_on_violation: options.fail_on_violation(),
        format_form_count: summary.format_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formats(input: &str) -> (usize, Vec<FormatToStringItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_format_to_strings(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect format to strings")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_aesthetic_directive_as_princ_to_string() {
        let source = "(format nil \"~A\" value)";
        let (count, violations) = formats(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].replacement, "princ-to-string");
        assert_eq!(slice(source, violations[0].argument_span), "value");
    }

    #[test]
    fn flags_standard_directive_as_prin1_to_string() {
        let (_, violations) = formats("(format nil \"~S\" x)");
        assert_eq!(violations[0].replacement, "prin1-to-string");
    }

    #[test]
    fn folds_directive_case() {
        assert_eq!(
            formats("(format nil \"~a\" x)").1[0].replacement,
            "princ-to-string"
        );
        assert_eq!(
            formats("(format nil \"~s\" x)").1[0].replacement,
            "prin1-to-string"
        );
    }

    #[test]
    fn preserves_a_compound_argument() {
        let source = "(format nil \"~A\" (compute x))";
        let (_, violations) = formats(source);
        assert_eq!(slice(source, violations[0].argument_span), "(compute x)");
    }

    #[test]
    fn does_not_flag_surrounding_text() {
        assert!(formats("(format nil \"value: ~A\" x)").1.is_empty());
        assert!(formats("(format nil \"~A~%\" x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_nil_destination() {
        // t / stream destinations return nil, not the string.
        assert!(formats("(format t \"~A\" x)").1.is_empty());
        assert!(formats("(format s \"~A\" x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_argument_count() {
        assert!(formats("(format nil \"~A\")").1.is_empty());
        assert!(formats("(format nil \"~A\" a b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_other_directives() {
        assert!(formats("(format nil \"~D\" x)").1.is_empty());
        assert!(formats("(format nil \"~%\" x)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_format() {
        let (_, violations) = formats("(defun f (x) (format nil \"~A\" x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(format nil \"~A\" x)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_format_to_strings(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect format to strings");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = formats("(format nil \"~A\" x)");
        let summary = summarize_format_to_strings(count, items);

        let quiet =
            evaluate_format_to_string_policy(FormatToStringPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_format_to_string_policy(FormatToStringPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
