//! Common Lisp unknown-`format`-directive detection: a `~` directive in a
//! *literal* control string whose dispatch character is not one of the
//! directives CLHS 22.3 defines.
//!
//! A control string is checked at run time, not at compile time, so
//! `(format t "~Q" x)` compiles cleanly and fails on whichever branch reaches
//! it. The dispatch character is the one thing about a directive that is
//! decidable without knowing anything about the arguments, which is why this
//! rule can be exact where an argument-counting rule cannot.
//!
//! # The table
//!
//! Every character below is a section title of CLHS 22.3, read from the
//! specification rather than from memory:
//!
//! | section | directives |
//! |---------|------------|
//! | 22.3.1 Basic Output | `~C` `~%` `~&` `~|` `~~` |
//! | 22.3.2 Radix Control | `~R` `~D` `~B` `~O` `~X` |
//! | 22.3.3 Floating-Point | `~F` `~E` `~G` `~$` |
//! | 22.3.4 Printer Operations | `~A` `~S` `~W` |
//! | 22.3.5 Pretty Printer | `~_` `~<` `~I` `~/` |
//! | 22.3.6 Layout Control | `~T` `~<` `~>` |
//! | 22.3.7 Control-Flow | `~*` `~[` `~]` `~{` `~}` `~?` |
//! | 22.3.8 Miscellaneous | `~(` `~)` `~P` |
//! | 22.3.9 Miscellaneous Pseudo | `~;` `~^` `~<newline>` |
//!
//! CLHS 22.3 says "The case of the directive character is ignored", so the
//! comparison folds case.
//!
//! # Where this rule declines to speak
//!
//! CLHS does not reserve the unlisted characters, so an implementation may
//! define its own. That is why the finding is a warning rather than an error,
//! and it is the reason the rule reports *what* it saw rather than asserting
//! the call will fail.
//!
//! Everything the shared scanner refuses to guess at, this rule inherits: an
//! unterminated `~` at the end of a string, and a `~/name` whose closing slash
//! never arrives, both end the scan and are not reported. Both are certainly
//! malformed and both are deliberately a false negative — see
//! [`crate::support::directives`].
//!
//! Scope: Common Lisp only. Emacs Lisp's `format` uses `%` directives, and
//! nothing here would mean anything applied to one.

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

/// Every dispatch character CLHS 22.3 defines, folded to lower case.
///
/// The two whitespace entries are the "Tilde Newline: Ignored Newline"
/// directive of 22.3.9.3 — the idiom that lets a long control string be broken
/// across source lines. `\r` is here because a file with CRLF line endings
/// spells that same directive with a carriage return first, and reporting it
/// would make this rule fire on line endings.
const STANDARD_DIRECTIVES: [char; 34] = [
    // 22.3.1 Basic Output
    'c', '%', '&', '|', '~', // 22.3.2 Radix Control
    'r', 'd', 'b', 'o', 'x', // 22.3.3 Floating-Point Printers
    'f', 'e', 'g', '$', // 22.3.4 Printer Operations
    'a', 's', 'w', // 22.3.5 Pretty Printer Operations
    '_', '<', 'i', '/', // 22.3.6 Layout Control
    't', '>', // 22.3.7 Control-Flow Operations
    '*', '[', ']', '{', '}', '?', // 22.3.8 Miscellaneous Operations
    '(', ')', 'p', // 22.3.9 Miscellaneous Pseudo-Operations
    ';', '^',
];

/// Whether `character` is a directive CLHS 22.3 defines.
fn is_standard(character: char) -> bool {
    // The ignored-newline directive, whose dispatch character is literal
    // whitespace rather than a letter.
    if character == '\n' || character == '\r' {
        return true;
    }
    STANDARD_DIRECTIVES.contains(&character.to_ascii_lowercase())
}

#[derive(Debug, Clone)]
pub struct FormatUnknownDirectiveItem {
    /// The span of the control string itself, not of the whole call.
    pub span: ByteSpan,
    /// The unknown directives, in order, spelled as they appear (`~Q`).
    pub unknown: Vec<String>,
}

impl Finding for FormatUnknownDirectiveItem {
    fn kind(&self) -> &'static str {
        "format-unknown-directive"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("unknown={}", self.unknown.join(","))]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("unknown", json!(self.unknown))]
    }

    /// The same sentence the `format-unknown-directive` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "format control string has no such directive: {}",
            self.unknown.join(", ")
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// One finding per control string rather than one per directive: a string with
/// three unknown directives is one mistake in one string, and three findings on
/// one span would say the same thing three times.
pub fn examine(
    view: &ExpressionView,
    control_string_count: &mut usize,
    violations: &mut Vec<FormatUnknownDirectiveItem>,
) {
    // The cheap disqualifiers first: a head this package knows, a literal in
    // the control slot, and a `~` somewhere in it. Nothing allocates until all
    // three hold.
    let Some(raw) = literal_control_string(view) else {
        return;
    };
    *control_string_count += 1;

    let control = resolve_escapes(raw);
    let unknown: Vec<String> = directives(&control)
        .filter(|directive| !is_standard(directive.character))
        .map(|directive| format!("~{}", directive.character))
        .collect();
    if unknown.is_empty() {
        return;
    }

    let Some(span) = control_string_span(view) else {
        return;
    };
    violations.push(FormatUnknownDirectiveItem { span, unknown });
}

/// Collects every `format`-family call with an unknown directive in one file,
/// with the number of literal control strings scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every directive here is standard" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_format_unknown_directive_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FormatUnknownDirectiveItem>> {
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

    fn report(input: &str) -> FileFindings<FormatUnknownDirectiveItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_format_unknown_directive_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build format unknown directive report")
    }

    /// The `(control_string_count, violations)` pair the report is built from.
    fn scanned(input: &str) -> (u64, Vec<FormatUnknownDirectiveItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "control_string_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("control_string_count in the summary");
        (count, report.findings)
    }

    fn unknown_of(input: &str) -> Vec<String> {
        scanned(input)
            .1
            .into_iter()
            .flat_map(|item| item.unknown)
            .collect()
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_directive_that_is_not_in_the_standard() {
        let (count, violations) = scanned("(format t \"~Q\" x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].unknown, vec!["~Q".to_owned()]);
    }

    #[test]
    fn flags_every_unknown_directive_in_one_string_as_one_finding() {
        let (_, violations) = scanned("(format t \"~Q~a~Z\" x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].unknown,
            vec!["~Q".to_owned(), "~Z".to_owned()]
        );
    }

    #[test]
    fn flags_the_other_format_family_heads_at_their_own_control_slots() {
        assert_eq!(unknown_of("(error \"~Q\" x)"), vec!["~Q".to_owned()]);
        assert_eq!(unknown_of("(warn \"~Q\" x)"), vec!["~Q".to_owned()]);
        assert_eq!(
            unknown_of("(cerror \"Retry.\" \"~Q\" x)"),
            vec!["~Q".to_owned()]
        );
        assert_eq!(
            unknown_of("(format-to-string \"~Q\" x)"),
            vec!["~Q".to_owned()]
        );
    }

    #[test]
    fn folds_the_head_case_and_the_package_qualifier() {
        assert_eq!(unknown_of("(FORMAT T \"~Q\" x)"), vec!["~Q".to_owned()]);
        assert_eq!(unknown_of("(cl:format t \"~Q\" x)"), vec!["~Q".to_owned()]);
    }

    #[test]
    fn finds_a_nested_call() {
        assert_eq!(
            unknown_of("(defun f (x) (when x (format t \"~Q\" x)))"),
            vec!["~Q".to_owned()]
        );
    }

    // -- near-miss negatives -------------------------------------------------

    /// The whole standard table, exercised as one control string per section.
    #[test]
    fn does_not_flag_any_standard_directive() {
        for control in [
            "~C~%~&~|~~",
            "~R~D~B~O~X",
            "~F~E~G~$",
            "~A~S~W",
            "~_~<abc~>~I",
            "~/pkg:fn/",
            "~T",
            "~*~[a~;b~]~{~a~}~?",
            "~(abc~)~P",
            "~;~^",
        ] {
            assert!(
                unknown_of(&format!("(format t \"{control}\" x)")).is_empty(),
                "{control} is standard"
            );
        }
    }

    #[test]
    fn does_not_flag_a_lowercase_spelling() {
        assert!(unknown_of("(format t \"~a~s~d~%\" x)").is_empty());
    }

    /// CLHS 22.3.9.3: a tilde before a newline is the ignored-newline
    /// directive, which is how a long control string is broken across lines.
    #[test]
    fn does_not_flag_an_ignored_newline() {
        assert!(unknown_of("(format t \"a very long ~\n            line ~a\" x)").is_empty());
        assert!(unknown_of("(format t \"a~:\n  b ~a\" x)").is_empty());
        assert!(unknown_of("(format t \"a~\r\n  b ~a\" x)").is_empty());
    }

    /// `~~` emits a literal tilde, so the character after it is ordinary text
    /// and is not a directive at all.
    #[test]
    fn does_not_flag_the_text_after_a_literal_tilde() {
        assert!(unknown_of("(format t \"100~~Q of them\")").is_empty());
    }

    /// The prefix-parameter traps. Each of these would produce a phantom
    /// unknown directive under a scanner that stopped at the tilde.
    #[test]
    fn does_not_flag_a_prefix_parameter_as_a_directive() {
        assert!(unknown_of("(format t \"~10,'0D\" n)").is_empty());
        assert!(unknown_of("(format t \"~5,'*d\" n)").is_empty());
        assert!(unknown_of("(format t \"~3,-4:@s\" x)").is_empty());
        assert!(unknown_of("(format t \"~,+4S\" x)").is_empty());
        assert!(unknown_of("(format t \"~vd\" w n)").is_empty());
        assert!(unknown_of("(format t \"~#[none~;one~:;many~]\" x)").is_empty());
    }

    /// `~/pkg:fn/` names a function; the name is not control-string text and
    /// its characters are not directives.
    #[test]
    fn does_not_flag_the_function_name_of_a_call_directive() {
        assert!(unknown_of("(format t \"~/my-pkg:print-thing/\" x)").is_empty());
        assert!(unknown_of("(format t \"~/qzy/\" x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_control_string() {
        let (count, violations) = scanned("(format t (banner) x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// The package-specific trap: a `~` sequence in an ordinary argument is a
    /// string being *printed*, not a control string being interpreted.
    #[test]
    fn does_not_parse_a_tilde_in_a_non_control_argument() {
        let (count, violations) = scanned("(format t \"~a\" \"~Q~Z\")");
        assert_eq!(count, 1, "only the control string is a candidate");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_head_that_is_not_a_format_operator() {
        let (count, violations) = scanned("(list \"~Q\" x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// `format-missing-destination`'s subject: the literal is in the
    /// *destination* slot, so the control slot holds `x` and there is no
    /// literal control string here at all. The two rules cannot both fire.
    #[test]
    fn does_not_reach_a_literal_in_the_destination_slot() {
        let (count, violations) = scanned("(format \"~Q\" x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_unterminated_directive() {
        assert!(unknown_of("(format t \"done ~\")").is_empty());
        assert!(unknown_of("(format t \"~/unclosed\")").is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn the_report_walk_skips_all_five_quote_shapes() {
        for source in [
            "'(format t \"~Q\" x)",
            "(quote (format t \"~Q\" x))",
            "`(format t \"~Q\" x)",
            "'(a ,(format t \"~Q\" x))",
        ] {
            assert!(unknown_of(source).is_empty(), "{source} is data");
        }
        // The one shape that is code again.
        assert_eq!(
            unknown_of("`(a ,(format t \"~Q\" x))"),
            vec!["~Q".to_owned()]
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(format t \"~Q\" x)", Dialect::Clojure).expect("parse");
        let report =
            build_format_unknown_directive_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build format unknown directive report");
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
        let source = "(defun f (x)\n  (format t \"~Q\" x))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "format-unknown-directive");
        assert_eq!(finding.json_fields(), vec![("unknown", json!(["~Q"]))]);
        assert_eq!(finding.text_columns(), vec!["unknown=~Q".to_owned()]);
        assert_eq!(
            finding.message(),
            "format control string has no such directive: ~Q"
        );
        let span = finding.span();
        assert_eq!(&source[span.start().get()..span.end().get()], "\"~Q\"");
    }

    #[test]
    fn the_summary_counts_every_control_string_scanned_not_only_the_flagged_ones() {
        let report = report("(format t \"~Q\" x)\n(format t \"~a\" y)\n");
        assert_eq!(report.summary, vec![("control_string_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
