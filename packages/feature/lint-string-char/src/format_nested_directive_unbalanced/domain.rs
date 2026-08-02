//! Common Lisp unbalanced-`format`-construct detection: a bracketing directive
//! in a *literal* control string that is never closed, closed by the wrong
//! partner, or closed without having been opened.
//!
//! CLHS 22.3.10.1 is unambiguous about this being a defect rather than a
//! stylistic complaint:
//!
//! > The case-conversion, conditional, iteration, and justification constructs
//! > can contain other formatting constructs by bracketing them. **These
//! > constructs must nest properly with respect to each other.**
//!
//! and it labels its own crossed-construct example `;Invalid!`.
//!
//! # The four pairs
//!
//! | construct | opens | closes | CLHS |
//! |-----------|-------|--------|------|
//! | conditional | `~[` | `~]` | 22.3.7.2 / 22.3.7.3 |
//! | iteration | `~{` | `~}` | 22.3.7.4 / 22.3.7.5 |
//! | justification, logical block | `~<` | `~>` | 22.3.6.2 / 22.3.6.3 |
//! | case conversion | `~(` | `~)` | 22.3.8.1 / 22.3.8.2 |
//!
//! Case conversion is in the table because CLHS 22.3.10.1 names it first among
//! the constructs that must nest, and because its own invalid example is a
//! `~(…~)` crossed with a `~[…~]`. Leaving it out would decline to report the
//! specification's own worked example.
//!
//! `~;` (clause separator) and `~^` (escape upward) appear *inside* these
//! constructs and open nothing, so they are ignored here rather than tracked.
//!
//! # What this rule is not
//!
//! It is not an argument-count check. `paredit-feature-lisp-analysis`'s
//! `format_directive_report` counts value-consuming directives against supplied
//! arguments and reports any control string containing `~{ ~[ ~? ~*` as
//! *indeterminate* rather than guessing. This rule never counts arguments and
//! never looks at the call's argument list at all; it reads one control string
//! as a bracket language. The two are disjoint: every string this rule can
//! report is one the other declines to score.
//!
//! # Where this rule declines to speak
//!
//! CLHS 22.3.10.1 also forbids a construct that opens in one control string and
//! closes in another through `~?`. That is not detectable from one literal —
//! the other string is a run-time argument — so a string that this rule reads
//! as unbalanced is reported on its own terms, and a `~?` in it does not
//! suppress the finding. A construct genuinely spanning a `~?` is invalid
//! anyway, by the same paragraph.
//!
//! The shared scanner's refusals are inherited, and they cut in one direction
//! only. An unterminated `~` at the end of a string, or a `~/name` with no
//! closing slash, ends the scan — so a *closing* directive after one of them is
//! never seen, and a construct still open at that point is reported as
//! unclosed. That is the right answer for `"~{~a~"`, where the iteration
//! genuinely never closes. It would be the wrong answer for a string whose
//! `~}` sits after a malformed `~/name`, but such a string is malformed
//! whichever way it is read, so there is no correct code in that gap.
//!
//! A trailing `~` after a construct has already closed is simply not seen:
//! `"~{~a~}~"` is balanced and is not reported.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    control_string_span, directives, for_each_evaluated_subview, literal_control_string,
    resolve_escapes,
};

/// The bracketing constructs of CLHS 22.3.10.1, as `(opener, closer)`.
const BRACKETS: [(char, char); 4] = [('[', ']'), ('{', '}'), ('<', '>'), ('(', ')')];

/// The closer that matches `opener`, or `None` if `opener` opens nothing.
fn closer_for(opener: char) -> Option<char> {
    BRACKETS
        .iter()
        .find(|(open, _)| *open == opener)
        .map(|(_, close)| *close)
}

/// Whether `character` closes some bracketing construct.
fn is_closer(character: char) -> bool {
    BRACKETS.iter().any(|(_, close)| *close == character)
}

/// The one byte scan that decides whether a control string is worth reading as
/// a bracket language at all.
///
/// A string with none of these eight characters anywhere in it — the common
/// case by a wide margin — is disqualified before a directive is scanned or
/// anything is allocated.
///
/// Purely a fast path, and *semantically transparent by construction*: a string
/// with no bracket character has no bracketing directive, so
/// [`first_imbalance`] would return `None` for it anyway. That is why deleting
/// this check changes no finding and no test — verified by mutation, which is
/// the answer a correct fast path must give. It is covered directly by
/// [`tests::the_bracket_disqualifier_admits_exactly_the_strings_with_a_bracket`]
/// instead, so it is not untested code.
fn mentions_a_bracket(raw: &str) -> bool {
    raw.bytes()
        .any(|byte| matches!(byte, b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b'(' | b')'))
}

/// What is wrong with the nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imbalance {
    /// A construct was opened and the string ended before it closed.
    Unclosed,
    /// A closing directive arrived with no construct open.
    Unopened,
    /// A closing directive arrived for a construct other than the innermost
    /// open one — CLHS 22.3.10.1's crossed-construct case.
    Mismatched,
}

impl Imbalance {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unclosed => "unclosed",
            Self::Unopened => "unopened",
            Self::Mismatched => "mismatched",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormatNestedDirectiveUnbalancedItem {
    /// The span of the control string itself, not of the whole call.
    pub span: ByteSpan,
    pub imbalance: Imbalance,
    /// The directive that could not be reconciled, spelled as `~{` or `~]`.
    pub directive: String,
}

impl Finding for FormatNestedDirectiveUnbalancedItem {
    /// The specific imbalance, so a report groups the three cases apart.
    fn kind(&self) -> &'static str {
        self.imbalance.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("imbalance={}", self.imbalance.label()),
            format!("directive={}", self.directive),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("imbalance", json!(self.imbalance.label())),
            ("directive", json!(self.directive)),
        ]
    }

    /// The same sentence the `format-nested-directive-unbalanced` lint rule
    /// writes, so a SARIF or JUnit consumer reading both sees one finding
    /// described one way.
    fn message(&self) -> String {
        match self.imbalance {
            Imbalance::Unclosed => format!(
                "format control string opens {} and never closes it",
                self.directive
            ),
            Imbalance::Unopened => format!(
                "format control string closes {} that was never opened",
                self.directive
            ),
            Imbalance::Mismatched => format!(
                "format control string closes {} against a different open construct",
                self.directive
            ),
        }
    }
}

/// The first imbalance in `control`, or `None` when every construct nests.
///
/// The *first* one only: once the bracket stack is wrong, everything after it
/// is a consequence rather than an independent mistake, and reporting the
/// cascade would bury the one directive a reader has to look at.
fn first_imbalance(control: &str) -> Option<(Imbalance, char)> {
    // `Vec::new` does not allocate; a string with no bracketing directive
    // therefore pays nothing here even after the byte scan lets it through.
    let mut open: Vec<char> = Vec::new();
    for directive in directives(control) {
        let character = directive.folded();
        if let Some(_closer) = closer_for(character) {
            open.push(character);
        } else if is_closer(character) {
            match open.pop() {
                None => return Some((Imbalance::Unopened, character)),
                Some(opener) => {
                    if closer_for(opener) != Some(character) {
                        return Some((Imbalance::Mismatched, character));
                    }
                }
            }
        }
    }
    open.pop().map(|opener| (Imbalance::Unclosed, opener))
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    control_string_count: &mut usize,
    violations: &mut Vec<FormatNestedDirectiveUnbalancedItem>,
) {
    // The cheap disqualifiers first: a head this package knows, a literal in
    // the control slot, a `~` in it, and a bracket character somewhere.
    let Some(raw) = literal_control_string(view) else {
        return;
    };
    *control_string_count += 1;

    if !mentions_a_bracket(raw) {
        return;
    }

    let control = resolve_escapes(raw);
    let Some((imbalance, character)) = first_imbalance(&control) else {
        return;
    };
    let Some(span) = control_string_span(view) else {
        return;
    };
    violations.push(FormatNestedDirectiveUnbalancedItem {
        span,
        imbalance,
        directive: format!("~{character}"),
    });
}

/// Collects every `format`-family call whose literal control string does not
/// nest, with the number of literal control strings scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every construct here nests" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_format_nested_directive_unbalanced_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FormatNestedDirectiveUnbalancedItem>> {
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

    fn report(input: &str) -> FileFindings<FormatNestedDirectiveUnbalancedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_format_nested_directive_unbalanced_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build format nested directive unbalanced report")
    }

    /// The `(control_string_count, violations)` pair the report is built from.
    fn scanned(input: &str) -> (u64, Vec<FormatNestedDirectiveUnbalancedItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "control_string_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("control_string_count in the summary");
        (count, report.findings)
    }

    /// The `(imbalance, directive)` a control string produces, wrapped in the
    /// plainest call that reaches the rule.
    fn verdict(control: &str) -> Option<(Imbalance, String)> {
        let (_, violations) = scanned(&format!("(format t \"{control}\" x)"));
        violations
            .into_iter()
            .next()
            .map(|item| (item.imbalance, item.directive))
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_an_iteration_that_is_never_closed() {
        let (count, violations) = scanned("(format t \"~{~a\" xs)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].imbalance, Imbalance::Unclosed);
        assert_eq!(violations[0].directive, "~{");
    }

    #[test]
    fn flags_each_construct_left_open() {
        assert_eq!(verdict("~[a"), Some((Imbalance::Unclosed, "~[".to_owned())));
        assert_eq!(verdict("~{a"), Some((Imbalance::Unclosed, "~{".to_owned())));
        assert_eq!(verdict("~<a"), Some((Imbalance::Unclosed, "~<".to_owned())));
        assert_eq!(verdict("~(a"), Some((Imbalance::Unclosed, "~(".to_owned())));
    }

    #[test]
    fn flags_a_close_with_nothing_open() {
        assert_eq!(verdict("a~]"), Some((Imbalance::Unopened, "~]".to_owned())));
        assert_eq!(verdict("a~}"), Some((Imbalance::Unopened, "~}".to_owned())));
        assert_eq!(verdict("a~>"), Some((Imbalance::Unopened, "~>".to_owned())));
        assert_eq!(verdict("a~)"), Some((Imbalance::Unopened, "~)".to_owned())));
    }

    #[test]
    fn flags_a_construct_closed_by_the_wrong_partner() {
        assert_eq!(
            verdict("~{~a~]"),
            Some((Imbalance::Mismatched, "~]".to_owned()))
        );
        assert_eq!(
            verdict("~[a~}"),
            Some((Imbalance::Mismatched, "~}".to_owned()))
        );
    }

    /// CLHS 22.3.10.1's own worked example, transcribed: a case-conversion
    /// construct opened inside one arm of a conditional and closed outside it.
    /// The specification labels it `;Invalid!`.
    #[test]
    fn flags_the_specifications_own_crossed_construct_example() {
        let (_, violations) = scanned("(format nil \"~:[abc~:@(def~;ghi~:@(jkl~]mno~)\" x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].imbalance, Imbalance::Mismatched);
        assert_eq!(violations[0].directive, "~]");
    }

    #[test]
    fn flags_the_other_format_family_heads_at_their_own_control_slots() {
        assert!(!scanned("(error \"~{~a\" xs)").1.is_empty());
        assert!(!scanned("(warn \"~{~a\" xs)").1.is_empty());
        assert!(!scanned("(cerror \"Retry.\" \"~{~a\" xs)").1.is_empty());
        assert!(!scanned("(format-to-string \"~{~a\" xs)").1.is_empty());
    }

    #[test]
    fn folds_the_head_case_and_the_package_qualifier() {
        assert!(!scanned("(FORMAT T \"~{~a\" xs)").1.is_empty());
        assert!(!scanned("(cl:format t \"~{~a\" xs)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_call() {
        assert!(
            !scanned("(defun f (xs) (format t \"~{~a\" xs))")
                .1
                .is_empty()
        );
    }

    // -- the fast path -------------------------------------------------------

    /// The disqualifier is a fast path, so no finding can distinguish it. What
    /// *is* checkable is that it never turns away a string the balance check
    /// would have had an opinion about.
    #[test]
    fn the_bracket_disqualifier_admits_exactly_the_strings_with_a_bracket() {
        for admitted in ["~{~a~}", "~[a~]", "~<a~>", "~(a~)", "plain [ text", "~a}"] {
            assert!(mentions_a_bracket(admitted), "{admitted}");
        }
        for turned_away in ["~a ~s", "~%~&", "~5,'0d", "", "no brackets here"] {
            assert!(!mentions_a_bracket(turned_away), "{turned_away}");
            assert_eq!(
                first_imbalance(turned_away),
                None,
                "{turned_away} must be balanced for the fast path to be sound"
            );
        }
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_balanced_construct() {
        assert_eq!(verdict("~{~a~}"), None);
        assert_eq!(verdict("~[none~;one~;many~]"), None);
        assert_eq!(verdict("~<a~;b~>"), None);
        assert_eq!(verdict("~(shout~)"), None);
    }

    #[test]
    fn does_not_flag_properly_nested_constructs() {
        assert_eq!(verdict("~{~[a~;b~]~}"), None);
        assert_eq!(verdict("~<~{~a~}~>"), None);
        assert_eq!(verdict("~(~[a~;b~]~)"), None);
        assert_eq!(verdict("~{~a~^, ~}"), None);
    }

    /// Modifiers decorate a closing directive without changing which construct
    /// it closes: `~:>` ends a logical block, `~:}` an iteration.
    #[test]
    fn does_not_flag_a_decorated_closer() {
        assert_eq!(verdict("~<~a~:>"), None);
        assert_eq!(verdict("~{~a~:}"), None);
        assert_eq!(verdict("~:[no~;yes~]"), None);
        assert_eq!(verdict("~@[~a~]"), None);
        assert_eq!(verdict("~#[none~;one~:;many~]"), None);
    }

    /// The pretty-printer's own idiom: a logical block with prefix and suffix
    /// strings given as `~<` parameters.
    #[test]
    fn does_not_flag_a_logical_block_with_prefix_parameters() {
        assert_eq!(verdict("~<(~;~@{~a~^ ~}~;)~:>"), None);
    }

    /// An ordinary bracket character is text, not a directive. Only a
    /// tilde-introduced one opens or closes anything.
    #[test]
    fn does_not_flag_bracket_characters_that_are_plain_text() {
        assert_eq!(verdict("[~a] {~a} (~a) <~a>"), None);
        assert_eq!(verdict("unmatched ( here ~a"), None);
        assert_eq!(verdict("a]b}c)d>~a"), None);
    }

    /// `~~` emits a literal tilde, so the character after it is text.
    #[test]
    fn does_not_flag_a_bracket_after_a_literal_tilde() {
        assert_eq!(verdict("~~{ and ~~} ~a"), None);
    }

    /// `~'{` makes the `{` a padding character for the directive that follows,
    /// not an iteration opener.
    #[test]
    fn does_not_flag_a_bracket_that_is_a_quoted_prefix_parameter() {
        assert_eq!(verdict("~5,'{d"), None);
        assert_eq!(verdict("~5,'}d"), None);
        assert_eq!(verdict("~{~5,'}d~}"), None);
    }

    /// `~/name/` names a function; brackets inside the name are part of the
    /// name and open nothing.
    #[test]
    fn does_not_flag_a_bracket_inside_a_call_directive_name() {
        assert_eq!(verdict("~/pkg:fn/~a"), None);
    }

    #[test]
    fn does_not_flag_a_computed_control_string() {
        let (count, violations) = scanned("(format t (banner) xs)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// The package-specific trap: a `~` sequence in an ordinary argument is a
    /// string being printed, not a control string being interpreted. This is
    /// also the `~?` shape — the inner control string is an argument, and it is
    /// the *outer* string's balance that is checked.
    #[test]
    fn does_not_parse_a_tilde_in_a_non_control_argument() {
        let (count, violations) = scanned("(format t \"~a\" \"~{~]\")");
        assert_eq!(count, 1, "only the control string is a candidate");
        assert!(violations.is_empty());

        let (_, violations) = scanned("(format t \"~?\" \"~{~a~}\" args)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_head_that_is_not_a_format_operator() {
        let (count, violations) = scanned("(list \"~{~a\" xs)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// An unterminated directive ends the scan without contributing a bracket
    /// of its own, so a construct that had already closed stays closed.
    #[test]
    fn does_not_flag_a_trailing_tilde_after_a_balanced_construct() {
        assert_eq!(verdict("~{~a~}~"), None);
        assert_eq!(verdict("~(a~)~"), None);
    }

    /// The other side of the same behaviour: a construct still open when the
    /// scan stops really is unclosed, and saying so is not a false positive.
    #[test]
    fn a_construct_still_open_at_an_unterminated_directive_is_unclosed() {
        assert_eq!(
            verdict("~{~a~"),
            Some((Imbalance::Unclosed, "~{".to_owned()))
        );
        assert_eq!(
            verdict("~{~/unclosed"),
            Some((Imbalance::Unclosed, "~{".to_owned()))
        );
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn the_report_walk_skips_the_four_data_quote_shapes() {
        for source in [
            "'(format t \"~{~a\" xs)",
            "(quote (format t \"~{~a\" xs))",
            "`(format t \"~{~a\" xs)",
            "'(a ,(format t \"~{~a\" xs))",
        ] {
            assert!(scanned(source).1.is_empty(), "{source} is data");
        }
    }

    /// The fifth shape, which is code again.
    #[test]
    fn an_unquote_inside_a_quasiquote_still_fires() {
        assert!(!scanned("`(a ,(format t \"~{~a\" xs))").1.is_empty());
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(format t \"~{~a\" xs)", Dialect::Clojure)
            .expect("parse");
        let report = build_format_nested_directive_unbalanced_report(
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
        let source = "(defun f (xs)\n  (format t \"~{~a\" xs))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "unclosed");
        assert_eq!(
            finding.json_fields(),
            vec![("imbalance", json!("unclosed")), ("directive", json!("~{"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["imbalance=unclosed".to_owned(), "directive=~{".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "format control string opens ~{ and never closes it"
        );
        let span = finding.span();
        assert_eq!(&source[span.start().get()..span.end().get()], "\"~{~a\"");
    }

    #[test]
    fn the_summary_counts_every_control_string_scanned_not_only_the_flagged_ones() {
        let report = report("(format t \"~{~a\" xs)\n(format t \"~a\" y)\n");
        assert_eq!(report.summary, vec![("control_string_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
