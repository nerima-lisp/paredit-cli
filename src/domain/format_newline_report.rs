//! Common Lisp `format`-newline detection: a `(format t "~%")` whose control
//! string is exactly the single newline directive `~%` and whose destination is
//! `t`. `format` with destination `t` writes to `*standard-output*` and returns
//! `nil`; `~%` unconditionally emits one `#\Newline`. That is exactly what
//! `(terpri)` does — write one newline to `*standard-output*` and return `nil` —
//! so `(format t "~%")` is `(terpri)`, stated directly and without a control
//! string to parse at run time.
//!
//! The rule is deliberately narrow for soundness:
//!
//!   - Only the `t` destination is matched. `t` always denotes
//!     `*standard-output*` (a stream), so `(terpri)` (also `*standard-output*`)
//!     is an exact match. An arbitrary destination expression could be a
//!     string with a fill pointer — a valid `format` destination but *not* a
//!     valid `terpri` argument — so a non-`t` destination is left alone.
//!   - Only `~%` (unconditional newline) is matched, never `~&` (`fresh-line`):
//!     `fresh-line` returns a generalized boolean, whereas `(format t "~&")`
//!     returns `nil`, so that rewrite would not preserve the return value.
//!   - The call must have no format arguments (exactly `format`, `t`, control);
//!     `format` evaluates every argument, so a trailing argument's evaluation
//!     would be lost by `(terpri)`.
//!
//! The fix rewrites the call as `(terpri)`, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `t` destination (no reader prefixes).
fn is_t_destination(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// Whether the control-string atom's source is exactly the `~%` directive.
/// `text` includes the surrounding quotes.
fn is_newline_control(text: &str) -> bool {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .is_some_and(|inner| inner == "~%")
}

#[derive(Debug, Clone)]
pub struct FormatNewlineItem {
    pub path: PathBuf,
    /// The span of the whole `(format t "~%")` form.
    pub span: ByteSpan,
}

#[derive(Debug)]
pub struct FormatNewlineSummary {
    pub format_form_count: usize,
    pub violations: Vec<FormatNewlineItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct FormatNewlinePolicyOptions {
    fail_on_violation: bool,
}

impl FormatNewlinePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct FormatNewlinePolicy {
    pub fail_on_violation: bool,
    pub format_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    format_form_count: &mut usize,
    violations: &mut Vec<FormatNewlineItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("format") {
        return;
    }
    *format_form_count += 1;

    // children: [format, destination, control] — no format arguments.
    if view.children.len() != 3 {
        return;
    }
    let destination = &view.children[1];
    let control = &view.children[2];
    if !is_t_destination(destination) {
        return;
    }
    if !atom_text(control).is_some_and(is_newline_control) {
        return;
    }

    violations.push(FormatNewlineItem {
        path: path.to_path_buf(),
        span: view.span,
    });
}

/// Collects every `(format t "~%")` across a whole file, along with the total
/// number of `format` forms scanned.
pub fn collect_format_newlines(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<FormatNewlineItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut format_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut format_form_count, &mut violations)
        });
    }
    Ok((format_form_count, violations))
}

pub fn summarize_format_newlines(
    format_form_count: usize,
    violations: Vec<FormatNewlineItem>,
) -> FormatNewlineSummary {
    FormatNewlineSummary {
        format_form_count,
        violations,
    }
}

pub fn evaluate_format_newline_policy(
    options: FormatNewlinePolicyOptions,
    summary: &FormatNewlineSummary,
) -> FormatNewlinePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    FormatNewlinePolicy {
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

    fn formats(input: &str) -> (usize, Vec<FormatNewlineItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_format_newlines(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect format newlines")
    }

    #[test]
    fn flags_format_t_newline() {
        let source = "(format t \"~%\")";
        let (count, violations) = formats(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn folds_head_case() {
        let (_, violations) = formats("(FORMAT T \"~%\")");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_nil_destination() {
        // (format nil "~%") returns the newline string, not terpri's nil.
        assert!(formats("(format nil \"~%\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_stream_destination() {
        // A stream/fill-pointer-string destination is not a valid terpri arg.
        assert!(formats("(format s \"~%\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_fresh_line() {
        // ~& is fresh-line, whose return value differs from format's nil.
        assert!(formats("(format t \"~&\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_control_with_extra_text() {
        assert!(formats("(format t \"~%~%\")").1.is_empty());
        assert!(formats("(format t \"done~%\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_call_with_arguments() {
        // format evaluates every argument; terpri would drop x's evaluation.
        assert!(formats("(format t \"~%\" x)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_format() {
        let (_, violations) = formats("(defun f () (format t \"~%\"))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(format t \"~%\")", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_format_newlines(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect format newlines");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = formats("(format t \"~%\")");
        let summary = summarize_format_newlines(count, items);

        let quiet =
            evaluate_format_newline_policy(FormatNewlinePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_format_newline_policy(FormatNewlinePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
