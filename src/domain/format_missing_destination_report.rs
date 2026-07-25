//! Common Lisp missing-`format`-destination detection: a `(format …)` call
//! whose first argument is a *string literal*. `format`'s first argument is the
//! destination — `nil` (return a string), `t` (write to `*standard-output*`),
//! or a stream — and the control string is the *second*. So `(format "~a" x)`
//! quietly uses the control string `"~a"` as the destination and `x` as the
//! control string, which is never what the author meant: a literal string can
//! never be a valid destination (that needs a string with a fill pointer, never
//! a constant). The fix is a missing `nil`/`t`, but no compiler flags the call
//! because it is technically well-formed.
//!
//! Only a string *literal* in the destination slot is flagged. A `nil`/`t`
//! symbol, a stream variable, or a `(make-…-stream)` form there is a correct
//! destination and is left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_child, for_each_subview, list_head};

#[derive(Debug, Clone)]
pub struct FormatMissingDestinationItem {
    pub path: PathBuf,
    /// The span of the whole `(format …)` form.
    pub span: ByteSpan,
    /// The string literal found in the destination slot (its source text).
    pub literal: String,
}

#[derive(Debug)]
pub struct FormatMissingDestinationSummary {
    pub format_call_count: usize,
    pub violations: Vec<FormatMissingDestinationItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct FormatMissingDestinationPolicyOptions {
    fail_on_violation: bool,
}

impl FormatMissingDestinationPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct FormatMissingDestinationPolicy {
    pub fail_on_violation: bool,
    pub format_call_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_format(
    view: &ExpressionView,
    path: &Path,
    format_call_count: &mut usize,
    violations: &mut Vec<FormatMissingDestinationItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("format")) {
        return;
    }
    *format_call_count += 1;

    // children[0] is `format`; children[1] is the destination argument.
    let Some(destination) = atom_child(view, 1) else {
        return;
    };
    if destination.starts_with('"') {
        violations.push(FormatMissingDestinationItem {
            path: path.to_path_buf(),
            span: view.span,
            literal: destination.to_owned(),
        });
    }
}

/// Collects every `format` call whose destination slot holds a string literal,
/// along with the total number of `format` calls scanned.
pub fn collect_format_missing_destinations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<FormatMissingDestinationItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut format_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_format(subview, path, &mut format_call_count, &mut violations)
        });
    }
    Ok((format_call_count, violations))
}

pub fn summarize_format_missing_destinations(
    format_call_count: usize,
    violations: Vec<FormatMissingDestinationItem>,
) -> FormatMissingDestinationSummary {
    FormatMissingDestinationSummary {
        format_call_count,
        violations,
    }
}

pub fn evaluate_format_missing_destination_policy(
    options: FormatMissingDestinationPolicyOptions,
    summary: &FormatMissingDestinationSummary,
) -> FormatMissingDestinationPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    FormatMissingDestinationPolicy {
        fail_on_violation: options.fail_on_violation(),
        format_call_count: summary.format_call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formats(input: &str) -> (usize, Vec<FormatMissingDestinationItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_format_missing_destinations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect format missing destinations")
    }

    #[test]
    fn flags_a_string_literal_destination() {
        let (count, violations) = formats("(format \"~a~%\" x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, "\"~a~%\"");
    }

    #[test]
    fn does_not_flag_nil_destination() {
        let (count, violations) = formats("(format nil \"~a\" x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_t_destination() {
        let (_, violations) = formats("(format t \"~a\" x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_stream_variable_destination() {
        let (_, violations) = formats("(format stream \"~a\" x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_stream_form_destination() {
        let (_, violations) = formats("(format (make-string-output-stream) \"~a\")");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = formats("(FORMAT \"done\")");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_a_nested_format() {
        let (_, violations) = formats("(when ready (format \"~a\" x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_non_format_heads() {
        let (count, violations) = formats("(list \"~a\" x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_format_with_no_arguments() {
        // Degenerate `(format)` has no destination slot to inspect.
        let (count, violations) = formats("(format)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(format \"~a\" x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_format_missing_destinations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect format missing destinations");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = formats("(format \"~a\" x)");
        let summary = summarize_format_missing_destinations(count, items);

        let quiet = evaluate_format_missing_destination_policy(
            FormatMissingDestinationPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_format_missing_destination_policy(
            FormatMissingDestinationPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
