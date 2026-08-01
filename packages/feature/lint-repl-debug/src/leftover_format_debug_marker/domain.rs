//! `leftover-format-debug-marker` detection: a `(format t "...")` or
//! `(format *standard-output* "...")` whose literal control-string argument
//! case-insensitively contains a debug marker token (`DEBUG`, `DBG`) as a
//! distinguishable word/prefix — `"~&DEBUG: ~a"`, `"[DBG] got here"`.
//!
//! Deliberately scoped to CL's `format` mini-language, and deliberately
//! disjoint from `leftover-print-debug`: that rule's Common Lisp head table
//! is `princ`/`print`/`prin1`/`pprint`, never `format`, so a leftover
//! `format` debug call is flagged by exactly one of the two rules, never
//! both.
//!
//! Only the two destinations that mean "write to `*standard-output*`" are
//! matched (`t` and the explicit special variable), and only when the
//! control string is a literal (not a computed expression) — otherwise there
//! is no text to inspect for a marker in the first place.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is};

use crate::support::{
    EvaluatedCandidate, OperatorScope, RemovalSafety, compute_evaluated_candidates,
};

const MARKERS: [&str; 2] = ["DEBUG", "DBG"];

/// Whether `view` is the bare `t` or `*standard-output*` destination — the
/// two spellings that write to `*standard-output*` — with no reader prefix.
fn is_standard_output_destination(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| {
            text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("*standard-output*")
        })
}

/// The control string's own text (quotes stripped, escapes left as written —
/// a marker search does not need to decode them), or `None` when `view` is
/// not a plain string literal.
fn control_string_text(view: &ExpressionView) -> Option<&str> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    text.strip_prefix('"').and_then(|t| t.strip_suffix('"'))
}

/// Whether `text` contains `DEBUG` or `DBG`, case-insensitively, at a word
/// boundary — preceded by the start of the string or a non-alphanumeric
/// character, so `"DEBUG: ~a"` and `"~&[DBG] ~a"` match but `"UNDEBUGGABLE"`
/// does not.
fn contains_debug_marker(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    MARKERS.iter().any(|marker| {
        let marker_bytes = marker.as_bytes();
        let mut start = 0;
        while let Some(offset) = upper[start..].find(marker) {
            let index = start + offset;
            let preceded_by_boundary = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
            if preceded_by_boundary {
                return true;
            }
            start = index + marker_bytes.len().max(1);
        }
        false
    })
}

#[derive(Debug, Clone)]
pub struct LeftoverFormatDebugMarkerItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub fix_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverFormatDebugMarkerSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverFormatDebugMarkerItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverFormatDebugMarkerPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverFormatDebugMarkerPolicyOptions {
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
pub struct LeftoverFormatDebugMarkerPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn examine(
    candidates: &[EvaluatedCandidate],
    scope: &OperatorScope<'_>,
    path: &Path,
    violations: &mut Vec<LeftoverFormatDebugMarkerItem>,
) {
    for candidate in candidates {
        let view = &candidate.view;
        let Some(head) = list_head(view) else {
            continue;
        };
        if !symbol_is(head, "format") {
            continue;
        }
        let Some(destination) = view.children.get(1) else {
            continue;
        };
        if !is_standard_output_destination(destination) {
            continue;
        }
        let Some(control) = view.children.get(2) else {
            continue;
        };
        let Some(text) = control_string_text(control) else {
            continue;
        };
        if !contains_debug_marker(text) {
            continue;
        }
        let locally_bound = candidate
            .head_symbol_span
            .is_some_and(|span| scope.symbol_span_is_locally_bound(span));
        let fix_span =
            (matches!(candidate.safety, RemovalSafety::Safe) && !locally_bound).then(|| {
                candidate
                    .removal_span
                    .expect("Safe candidate carries a removal span")
            });
        violations.push(LeftoverFormatDebugMarkerItem {
            path: path.to_path_buf(),
            span: view.span,
            fix_span,
        });
    }
}

pub fn collect_leftover_format_debug_marker(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverFormatDebugMarkerItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }
    let candidates = compute_evaluated_candidates(&tree.root_view());
    let mut violations = Vec::new();
    examine(
        &candidates,
        &OperatorScope::standalone(dialect, tree),
        path,
        &mut violations,
    );
    Ok((candidates.len(), violations))
}

#[must_use]
pub const fn summarize_leftover_format_debug_marker(
    scanned_form_count: usize,
    violations: Vec<LeftoverFormatDebugMarkerItem>,
) -> LeftoverFormatDebugMarkerSummary {
    LeftoverFormatDebugMarkerSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_format_debug_marker_policy(
    options: LeftoverFormatDebugMarkerPolicyOptions,
    summary: &LeftoverFormatDebugMarkerSummary,
) -> LeftoverFormatDebugMarkerPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverFormatDebugMarkerPolicy {
        fail_on_violation: options.fail_on_violation(),
        scanned_form_count: summary.scanned_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverFormatDebugMarkerItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_format_debug_marker(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_format_debug_marker");
        violations
    }

    #[test]
    fn flags_a_debug_marker_with_t_destination() {
        assert_eq!(
            violations_in("(format t \"~&DEBUG: ~a\" x)", Dialect::CommonLisp).len(),
            1
        );
    }

    #[test]
    fn flags_a_dbg_marker_with_standard_output_destination() {
        assert_eq!(
            violations_in(
                "(format *standard-output* \"[DBG] ~a\" x)",
                Dialect::CommonLisp
            )
            .len(),
            1
        );
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(
            violations_in("(format t \"debug: ~a\" x)", Dialect::CommonLisp).len(),
            1
        );
    }

    #[test]
    fn does_not_flag_a_format_without_a_marker() {
        assert!(violations_in("(format t \"~a\" x)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_a_marker_embedded_mid_word() {
        assert!(violations_in("(format t \"UNDEBUGGABLE\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_a_non_standard_output_destination() {
        assert!(violations_in("(format nil \"DEBUG ~a\" x)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(format *error-output* \"DEBUG\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_a_non_literal_control_string() {
        assert!(violations_in("(format t control-var x)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_double_flag_with_leftover_print_debug() {
        // `format` is not in `leftover-print-debug`'s Common Lisp head table
        // at all — this test documents that disjointness rather than testing
        // this rule's own logic.
        use crate::leftover_print_debug::domain::heads_for;
        assert!(!heads_for(Dialect::CommonLisp).contains(&"format"));
    }

    #[test]
    fn the_last_form_of_a_body_gets_no_fix() {
        let violations = violations_in("(defun f () (format t \"DEBUG\"))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_none());
    }

    #[test]
    fn a_non_last_body_form_gets_a_fix_that_leaves_valid_source() {
        let input = "(defun f ()\n  (format t \"DEBUG: ~a\" 1)\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) = collect_leftover_format_debug_marker(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\n  (+ 1 2))");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_quoted_shape_is_not_flagged() {
        assert!(violations_in("'(format t \"DEBUG\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_marker_inside_an_unrelated_string_argument_is_not_flagged() {
        // The marker text appears only in an *argument*, not the control
        // string itself.
        assert!(violations_in("(format t \"~a\" \"DEBUG\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_string_with_a_backslash_and_embedded_quote_is_handled_without_corruption() {
        let input = "(defun f ()\n  (format t \"DEBUG \\\"x\\\" \\\\ end\")\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) = collect_leftover_format_debug_marker(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect");
        assert_eq!(violations.len(), 1);
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\n  (+ 1 2))");
    }

    #[test]
    fn a_reader_conditional_wrapped_call_is_opaque() {
        assert!(violations_in("#+sbcl (format t \"DEBUG\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        let violations = violations_in(
            "(defun f () (format t \"DEBUG\") (+ 1 2))",
            Dialect::CommonLisp,
        );
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_some());
    }

    #[test]
    fn a_crlf_file_computes_a_correct_fix_span() {
        let input = "(defun f ()\r\n  (format t \"DEBUG\")\r\n  (+ 1 2))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) = collect_leftover_format_debug_marker(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\r\n  (+ 1 2))\r\n");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_call_to_a_locally_bound_flet_operator_is_reported_with_no_fix() {
        // The head is the file's own `flet` function/macro, not the
        // dialect's `format` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun f ()\n  (flet ((format (d c) (list d c)))\n    (format t \"DEBUG: here\")\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(shadowed.len(), 1, "the finding is still reported");
        assert!(
            shadowed[0].fix_span.is_none(),
            "a shadowed operator must carry no fix"
        );

        // The same shape with the `flet` binding renamed is the
        // ordinary case, and still gets its fix — without this the assertion
        // above would pass for the wrong reason.
        let unshadowed = violations_in(
            "(defun f ()\n  (flet ((render (d c) (list d c)))\n    (format t \"DEBUG: here\")\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].fix_span.is_some());
    }

    #[test]
    fn a_backquoted_format_shape_without_unquote_is_not_flagged() {
        // A backquote with no unquote is unevaluated data, so the shape
        // inside it is a list to build, not a call to remove.
        assert!(violations_in("`(a (format t \"DEBUG: here\") b)", Dialect::CommonLisp).is_empty());
    }
}
