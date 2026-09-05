//! Common Lisp `~%~&` detection: a fresh-line directive that immediately
//! follows an unconditional newline in a *literal* control string, where it can
//! do nothing.
//!
//! # What CLHS actually says, and what it does not
//!
//! - **`~%`** (22.3.1.2): "This outputs a #\Newline character, thereby
//!   terminating the current output line and beginning a new one."
//!   Unconditional.
//! - **`~&`** (22.3.1.3): "Unless it can be determined that the output stream
//!   is already at the beginning of a line, this outputs a newline."
//!   Conditional.
//!
//! So the order is the whole rule, and only one of the two orders is a defect:
//!
//! | written | second directive | verdict |
//! |---------|------------------|---------|
//! | `~%~&` | `~&`, after a newline was just written | **reported** |
//! | `~&~%` | `~%`, which always writes | **not reported** |
//!
//! `~&~%` is not redundant and is not reported. It is the ordinary "start a
//! fresh line, then leave a blank one" idiom, and the `~%` in it emits a
//! newline every single time. A rule that reported it would fire on correct,
//! deliberate, and extremely common code.
//!
//! # The hedge in `~&`, and why the finding survives it
//!
//! `~&` says "unless it *can be determined*" — an implementation that cannot
//! tell where the stream's cursor is emits a newline anyway. That does not
//! rescue `~%~&`, because it makes the pair mean one of exactly two things:
//!
//!   - the position is known (the usual case, and what every implementation
//!     manages for a stream it has just written a newline to), and the `~&`
//!     does nothing; or
//!   - the position is not known, and the pair emits two newlines — which is
//!     spelled `~%~%`.
//!
//! Either way the string does not say what it appears to say, which is what
//! this rule reports. It is a warning, not an error: nothing here is undefined
//! behaviour, and the finding is about the string being misleading.
//!
//! # Narrowness
//!
//! Only an *undecorated* `~%` immediately followed by an *undecorated* `~&` is
//! reported, and only when the two abut in the resolved control string. CLHS
//! 22.3.1.3 makes `~n&` call `fresh-line` and then emit `n-1` further newlines,
//! so `~%~2&` genuinely emits a newline and is not redundant, while `~0&` does
//! nothing at all anywhere — a different complaint than this one. CLHS 22.3.1.2
//! likewise gives `~n%` its own behaviour, and `~0%` emits nothing, so `~0%~&`
//! is not redundant either. Requiring both directives to be bare keeps all of
//! those out.
//!
//! # Not `format-newline`
//!
//! `format-newline` fires on a call that is exactly `(format t "~%")` — three
//! children, destination `t`, and a control string of exactly the two
//! characters `~%`. That string contains no `&`, so it cannot reach this rule,
//! and a string that reaches this rule is at least four characters long and
//! cannot be that one. The two triggers are disjoint by construction.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    Directive, control_string_span, directives, for_each_evaluated_subview, literal_control_string,
    resolve_escapes,
};

/// Whether `directive` is a bare `~%`: no prefix parameter, no modifier.
const fn is_bare_newline(directive: Directive) -> bool {
    directive.character == '%' && !directive.decorated
}

/// Whether `directive` is a bare `~&`: no prefix parameter, no modifier.
const fn is_bare_fresh_line(directive: Directive) -> bool {
    directive.character == '&' && !directive.decorated
}

#[derive(Debug, Clone)]
pub struct FormatPercentAmpersandAdjacentRedundancyItem {
    /// The span of the control string itself, not of the whole call.
    pub span: ByteSpan,
    /// How many `~%~&` pairs the control string contains.
    pub occurrences: usize,
}

impl Finding for FormatPercentAmpersandAdjacentRedundancyItem {
    fn kind(&self) -> &'static str {
        "format-percent-ampersand-adjacent-redundancy"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("occurrences={}", self.occurrences)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("occurrences", json!(self.occurrences))]
    }

    fn message(&self) -> String {
        "~& directly after ~% is already at the start of a line; drop it, or write ~%~% for a blank line"
            .to_owned()
    }
}

pub fn examine(
    view: &ExpressionView,
    control_string_count: &mut usize,
    violations: &mut Vec<FormatPercentAmpersandAdjacentRedundancyItem>,
) {
    // The cheap disqualifiers first: a head this package knows, a literal in
    // the control slot, and a `~` in it.
    let Some(raw) = literal_control_string(view) else {
        return;
    };
    *control_string_count += 1;

    // One more byte scan before anything allocates. A `~&` needs an `&`, and
    // the great majority of control strings do not contain one.
    if !raw.contains('&') {
        return;
    }

    let control = resolve_escapes(raw);
    let mut occurrences = 0;
    let mut previous: Option<Directive> = None;
    for directive in directives(&control) {
        // Adjacency is positional, not merely sequential: `~% ~&` also has a
        // no-op `~&`, but a reader can defend what that string says. Only the
        // directly abutting pair is reported.
        if previous.is_some_and(|earlier| {
            is_bare_newline(earlier)
                && is_bare_fresh_line(directive)
                && earlier.end == directive.start
        }) {
            occurrences += 1;
        }
        previous = Some(directive);
    }
    if occurrences == 0 {
        return;
    }

    let Some(span) = control_string_span(view) else {
        return;
    };
    violations.push(FormatPercentAmpersandAdjacentRedundancyItem { span, occurrences });
}

/// Collects every `format`-family call whose literal control string contains a
/// `~%~&` in one file, with the number of literal control strings scanned as
/// the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_format_percent_ampersand_adjacent_redundancy_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FormatPercentAmpersandAdjacentRedundancyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("control_string_count", json!(0))],
        ));
    }

    let mut control_string_count = 0;
    let mut violations = Vec::new();
    for_each_evaluated_subview(&tree.root_view(), |subview| {
        examine(subview, &mut control_string_count, &mut violations);
    });

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("control_string_count", json!(control_string_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<FormatPercentAmpersandAdjacentRedundancyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_format_percent_ampersand_adjacent_redundancy_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build format percent ampersand adjacent redundancy report")
    }

    fn scanned(input: &str) -> (u64, Vec<FormatPercentAmpersandAdjacentRedundancyItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "control_string_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("control_string_count in the summary");
        (count, report.findings)
    }

    fn fires(input: &str) -> bool {
        !scanned(input).1.is_empty()
    }

    /// The control string alone, wrapped in the plainest call that reaches the
    /// rule.
    fn control_fires(control: &str) -> bool {
        fires(&format!("(format t \"{control}\" x)"))
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_fresh_line_directly_after_a_newline() {
        let (count, violations) = scanned("(format t \"done~%~&next\" x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].occurrences, 1);
    }

    #[test]
    fn counts_every_pair_in_one_string_as_one_finding() {
        let (_, violations) = scanned("(format t \"a~%~&b~%~&c\")");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].occurrences, 2);
    }

    #[test]
    fn flags_the_other_format_family_heads_at_their_own_control_slots() {
        assert!(fires("(error \"boom~%~&\" x)"));
        assert!(fires("(warn \"careful~%~&\" x)"));
        assert!(fires("(cerror \"Retry.\" \"boom~%~&\" x)"));
        assert!(fires("(format-to-string \"boom~%~&\" x)"));
    }

    #[test]
    fn folds_the_head_case_and_the_package_qualifier() {
        assert!(fires("(FORMAT T \"a~%~&b\")"));
        assert!(fires("(cl:format t \"a~%~&b\")"));
    }

    /// The escape resolution matters: what `format` receives is `~%~&`, even
    /// though the source has a backslash between the two directives.
    #[test]
    fn flags_a_pair_that_is_only_adjacent_after_escapes_resolve() {
        assert!(fires("(format t \"a~%\\~&b\")"));
    }

    #[test]
    fn finds_a_nested_call() {
        assert!(fires("(defun f (x) (when x (format t \"a~%~&b\")))"));
    }

    // -- the refuted half: `~&~%` is not redundant ---------------------------

    /// CLHS 22.3.1.2 makes `~%` unconditional, so the `~%` of a `~&~%` always
    /// emits. This is the ordinary "fresh line, then a blank one" idiom, and
    /// reporting it would be a false positive on correct code.
    #[test]
    fn does_not_flag_the_reverse_order() {
        assert!(!control_fires("~&~%"));
        assert!(!control_fires("~&~%Report follows:~%"));
    }

    // -- near-miss negatives -------------------------------------------------

    /// CLHS 22.3.1.3: `~n&` emits `n-1` newlines beyond the fresh-line, so a
    /// decorated `~&` after a `~%` is not a no-op.
    #[test]
    fn does_not_flag_a_decorated_fresh_line() {
        assert!(!control_fires("~%~2&"));
        assert!(!control_fires("~%~0&"));
        assert!(!control_fires("~%~v&"));
        assert!(!control_fires("~%~:&"));
    }

    /// `~0%` emits nothing, so a `~&` after it is not guaranteed a no-op.
    #[test]
    fn does_not_flag_a_decorated_newline() {
        assert!(!control_fires("~2%~&"));
        assert!(!control_fires("~0%~&"));
        assert!(!control_fires("~v%~&"));
    }

    #[test]
    fn does_not_flag_a_pair_that_is_not_adjacent() {
        assert!(!control_fires("~% ~&"));
        assert!(!control_fires("~%text~&"));
        assert!(!control_fires("~%~a~&"));
    }

    #[test]
    fn does_not_flag_either_directive_on_its_own() {
        assert!(!control_fires("~%"));
        assert!(!control_fires("~&"));
        assert!(!control_fires("~&text~&"));
        assert!(!control_fires("~%~%"));
    }

    /// `~~` is the literal-tilde directive, so the five source characters
    /// `~~%~&` are `~~`, an ordinary `%`, and then `~&` — no `~%` at all. A
    /// substring search for `~%~&` reports this; a directive scan does not.
    #[test]
    fn does_not_flag_a_literal_tilde_followed_by_a_percent() {
        assert!(!control_fires("~~%~&"));
    }

    /// `~5,'%d` makes the `%` a padding character, not a newline directive.
    #[test]
    fn does_not_flag_a_percent_that_is_a_quoted_prefix_parameter() {
        assert!(!control_fires("~5,'%d~&"));
    }

    #[test]
    fn does_not_flag_a_computed_control_string() {
        let (count, violations) = scanned("(format t (banner) x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// The package-specific trap: a `~` sequence in an ordinary argument is a
    /// string being printed, not a control string being interpreted.
    #[test]
    fn does_not_parse_a_tilde_in_a_non_control_argument() {
        let (count, violations) = scanned("(format t \"~a\" \"~%~&\")");
        assert_eq!(count, 1, "only the control string is a candidate");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_head_that_is_not_a_format_operator() {
        let (count, violations) = scanned("(list \"~%~&\" x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// `format-newline`'s subject is a control string of exactly `~%`, which
    /// has no `&` in it and so can never reach this rule.
    #[test]
    fn does_not_overlap_with_format_newline() {
        let (count, violations) = scanned("(format t \"~%\")");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn the_report_walk_skips_the_four_data_quote_shapes() {
        for source in [
            "'(format t \"a~%~&b\")",
            "(quote (format t \"a~%~&b\"))",
            "`(format t \"a~%~&b\")",
            "'(a ,(format t \"a~%~&b\"))",
        ] {
            assert!(!fires(source), "{source} is data");
        }
    }

    /// The fifth shape, which is code again.
    #[test]
    fn an_unquote_inside_a_quasiquote_still_fires() {
        assert!(fires("`(a ,(format t \"a~%~&b\"))"));
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(format t \"a~%~&b\")", Dialect::Clojure)
            .expect("parse");
        let report = build_format_percent_ampersand_adjacent_redundancy_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("control_string_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(format t \"~a\" x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_points_at_the_control_string() {
        let source = "(defun f ()\n  (format t \"a~%~&b\"))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "format-percent-ampersand-adjacent-redundancy"
        );
        assert_eq!(finding.json_fields(), vec![("occurrences", json!(1))]);
        assert_eq!(finding.text_columns(), vec!["occurrences=1".to_owned()]);
        let span = finding.span();
        assert_eq!(&source[span.start().get()..span.end().get()], "\"a~%~&b\"");
    }

    #[test]
    fn the_summary_counts_every_control_string_scanned_not_only_the_flagged_ones() {
        let report = report("(format t \"a~%~&b\")\n(format t \"~a\" y)\n");
        assert_eq!(report.summary, vec![("control_string_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
