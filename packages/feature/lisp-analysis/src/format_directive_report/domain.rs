//! `format` control strings: which directives they contain, and whether the
//! call supplies the arguments they consume.
//!
//! A control string is a program in a second language, carried inside a string
//! literal that every tree-shaped analysis treats as one opaque atom. The
//! classic bug is an argument-count mismatch — `(format t "~A ~A" x)` compiles
//! cleanly and signals at run time, on whichever branch reaches it.
//!
//! Counting is deliberately conservative. `~A` consumes one argument and `~%`
//! consumes none, both unconditionally, so those are provable. Iteration
//! (`~{…~}`), conditionals (`~[…~]`), and indirection (`~?`) consume a number
//! that depends on the arguments themselves, so a control string containing any
//! of them is reported as *indeterminate* rather than guessed at. An
//! indeterminate finding never contributes a mismatch: reporting a false
//! mismatch on working code would make the whole report unusable.

use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head};
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// What the analysis concluded about one `format` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Directives and arguments agree.
    Balanced,
    /// The control string consumes more arguments than the call supplies.
    TooFewArguments,
    /// The call supplies arguments no directive consumes.
    TooManyArguments,
    /// The control string contains a directive whose appetite depends on the
    /// arguments, so no count is provable.
    Indeterminate,
}

impl Verdict {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::TooFewArguments => "too-few-arguments",
            Self::TooManyArguments => "too-many-arguments",
            Self::Indeterminate => "indeterminate",
        }
    }

    #[must_use]
    pub const fn is_mismatch(self) -> bool {
        matches!(self, Self::TooFewArguments | Self::TooManyArguments)
    }
}

/// One `format`-family call with a literal control string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatCall {
    pub verdict: Verdict,
    /// The control string as written, without its quotes.
    pub control: String,
    /// How many arguments the directives consume, when that is provable.
    pub consumed: Option<usize>,
    /// How many argument forms the call supplies after the control string.
    pub supplied: usize,
    /// Every directive character in the string, in order, so a consumer can
    /// see what was counted without re-parsing.
    pub directives: Vec<String>,
    pub span: ByteSpan,
}

impl Finding for FormatCall {
    fn kind(&self) -> &'static str {
        self.verdict.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!(
                "consumed={}",
                self.consumed
                    .map_or_else(|| "?".to_owned(), |count| count.to_string())
            ),
            format!("supplied={}", self.supplied),
            format!("directives={}", self.directives.join(",")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("verdict", json!(self.verdict.label())),
            ("control", json!(self.control)),
            ("consumed", json!(self.consumed)),
            ("supplied", json!(self.supplied)),
            ("directives", json!(self.directives)),
        ]
    }
}

/// The `format`-family operators whose control string is a fixed argument.
///
/// `format` takes a destination first; the rest take the control string
/// immediately. Getting that offset wrong would shift every count by one.
const FORMAT_HEADS: [(&str, usize); 5] = [
    ("format", 2),
    ("error", 1),
    ("warn", 1),
    ("cerror", 2),
    ("format-to-string", 1),
];

#[must_use]
pub fn build_format_directive_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<FormatCall> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut findings = Vec::new();

    if modelled {
        collect(&tree.root_view(), source, &mut findings);
    }

    let mismatched = findings
        .iter()
        .filter(|call: &&FormatCall| call.verdict.is_mismatch())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        vec![("mismatch_count", json!(mismatched))],
    )
}

fn collect(view: &ExpressionView, source: &str, findings: &mut Vec<FormatCall>) {
    if let Some(call) = format_call(view, source) {
        findings.push(call);
    }
    for child in &view.children {
        collect(child, source, findings);
    }
}

fn format_call(view: &ExpressionView, source: &str) -> Option<FormatCall> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    let control_index = FORMAT_HEADS
        .iter()
        .find(|(name, _)| common_lisp_operator_head_eq(head, name))
        .map(|(_, index)| *index)?;

    let control_view = view.children.get(control_index)?;
    let control = string_literal(control_view, source)?;
    let (consumed, indeterminate) = count_directives(&control);
    let supplied = view.children.len().saturating_sub(control_index + 1);

    let verdict = if indeterminate {
        Verdict::Indeterminate
    } else if consumed > supplied {
        Verdict::TooFewArguments
    } else if consumed < supplied {
        Verdict::TooManyArguments
    } else {
        Verdict::Balanced
    };

    Some(FormatCall {
        verdict,
        directives: directive_names(&control),
        consumed: (!indeterminate).then_some(consumed),
        supplied,
        control,
        span: view.span,
    })
}

/// The contents of a string literal node, with `\"` and `\\` resolved.
///
/// Returns `None` for anything that is not a literal, which is the point: a
/// computed control string is not analyzable, and a report that guessed at one
/// would be reporting on a string that does not exist.
fn string_literal(view: &ExpressionView, source: &str) -> Option<String> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = source.get(view.span.start().get()..view.span.end().get())?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;

    let mut resolved = String::with_capacity(inner.len());
    let mut escaped = false;
    for character in inner.chars() {
        if escaped {
            resolved.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            resolved.push(character);
        }
    }
    Some(resolved)
}

/// How many arguments a control string consumes, and whether that is provable.
///
/// The directive table is split by appetite rather than by purpose, because
/// appetite is the only thing this function is deciding.
fn count_directives(control: &str) -> (usize, bool) {
    let mut consumed = 0;
    let mut indeterminate = false;

    for (character, _) in directives(control) {
        match character.to_ascii_lowercase() {
            // Consume exactly one argument.
            'a' | 's' | 'd' | 'b' | 'o' | 'x' | 'r' | 'c' | 'f' | 'e' | 'g' | '$' | 'w' | 'p' => {
                consumed += 1;
            }
            // Consume none: pure output or layout.
            '%' | '&' | '|' | '~' | '\n' | 't' | '<' | '>' | '(' | ')' => {}
            // Appetite depends on the arguments. `~*` moves the argument
            // pointer, so even the *position* stops being knowable.
            '{' | '}' | '[' | ']' | '?' | '*' | '^' | ';' => indeterminate = true,
            // An unrecognised directive may be an implementation extension.
            // Refusing to count is the honest answer.
            _ => indeterminate = true,
        }
    }

    (consumed, indeterminate)
}

fn directive_names(control: &str) -> Vec<String> {
    directives(control)
        .map(|(character, prefix)| format!("~{prefix}{character}"))
        .collect()
}

/// Every `~`-directive in a control string, as its dispatch character and the
/// prefix parameters and modifiers that preceded it.
///
/// The prefix scan is what keeps `~10,'0D` from reading as a `~1` followed by
/// stray text: a directive's character is the first one after its parameters,
/// not the one after the tilde.
fn directives(control: &str) -> impl Iterator<Item = (char, String)> + '_ {
    let mut characters = control.char_indices().peekable();
    std::iter::from_fn(move || {
        loop {
            let (_, character) = characters.next()?;
            if character != '~' {
                continue;
            }
            let mut prefix = String::new();
            loop {
                let (_, next) = characters.peek().copied()?;
                if next.is_ascii_digit() || matches!(next, ',' | ':' | '@' | '\'' | '#' | 'v' | 'V')
                {
                    prefix.push(next);
                    characters.next();
                    // A `'` parameter quotes the character after it, which may
                    // itself look like a directive character.
                    if next == '\'' {
                        if let Some((_, quoted)) = characters.next() {
                            prefix.push(quoted);
                        }
                    }
                    continue;
                }
                characters.next();
                return Some((next, prefix));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<FormatCall> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_format_directive_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn verdict(source: &str) -> Verdict {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].verdict
    }

    #[test]
    fn matching_directives_and_arguments_are_balanced() {
        assert_eq!(verdict("(format t \"~A ~A\" x y)"), Verdict::Balanced);
    }

    #[test]
    fn a_missing_argument_is_reported() {
        assert_eq!(verdict("(format t \"~A ~A\" x)"), Verdict::TooFewArguments);
    }

    #[test]
    fn a_surplus_argument_is_reported() {
        assert_eq!(verdict("(format t \"~A\" x y)"), Verdict::TooManyArguments);
    }

    #[test]
    fn a_directive_that_consumes_nothing_is_not_counted() {
        assert_eq!(verdict("(format t \"~A~%\" x)"), Verdict::Balanced);
    }

    #[test]
    fn iteration_makes_the_count_indeterminate_rather_than_wrong() {
        assert_eq!(verdict("(format t \"~{~A~}\" xs)"), Verdict::Indeterminate);
    }

    #[test]
    fn an_indeterminate_call_is_never_counted_as_a_mismatch() {
        let report = report("(format t \"~{~A~}\" a b c)");
        assert_eq!(report.summary, vec![("mismatch_count", json!(0))]);
    }

    #[test]
    fn error_takes_its_control_string_first_because_it_has_no_destination() {
        assert_eq!(verdict("(error \"~A\" x)"), Verdict::Balanced);
    }

    #[test]
    fn a_prefix_parameter_does_not_hide_the_directive_character() {
        // `~10,'0D` is one directive consuming one argument, not several.
        assert_eq!(verdict("(format t \"~10,'0D\" n)"), Verdict::Balanced);
    }

    #[test]
    fn a_computed_control_string_is_not_analyzed() {
        let report = report("(format t (control-string) x)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_control_string_early() {
        assert_eq!(
            verdict("(format t \"~A \\\"~A\\\"\" x y)"),
            Verdict::Balanced
        );
    }

    #[test]
    fn the_directives_are_reported_in_order() {
        let report = report("(format t \"~A~%~S\" x y)");
        assert_eq!(
            report.findings[0].directives,
            vec!["~A".to_owned(), "~%".to_owned(), "~S".to_owned()]
        );
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(println \"~A\" x)", Dialect::Clojure).expect("parse");
        let report = build_format_directive_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
