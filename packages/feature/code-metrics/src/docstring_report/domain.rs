//! Definitions with no docstring, and docstrings that describe a lambda list
//! the definition does not have.
//!
//! The missing-docstring half is the obvious one and the less useful one: it is
//! a policy question, and a project either wants documentation on exports or
//! does not.
//!
//! The stale half is where the value is. A docstring that names `count` after
//! the parameter was renamed to `limit` is worse than no docstring at all — it
//! is a confident wrong answer, it survives every rename this tool performs
//! (renaming does not touch string contents, by design), and nothing else in
//! the toolchain will ever notice. Checking it is a matter of reading the
//! lambda list and looking for parameter names the prose does not mention, and
//! prose names the lambda list does not have.
//!
//! Both directions are reported, and they mean different things. A parameter
//! the docstring never mentions is *undocumented*. A name the docstring
//! mentions that is not a parameter is *stale* — the stronger signal, because
//! prose does not invent parameter names by accident.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// What is wrong with one definition's documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocstringIssue {
    /// No docstring at all.
    Missing,
    /// The docstring names something that is not a parameter. Almost always a
    /// rename the prose did not follow.
    StaleParameter,
    /// A parameter the docstring never mentions.
    UndocumentedParameter,
    /// A docstring that is present and consistent. Reported so the denominator
    /// is visible.
    Documented,
}

impl DocstringIssue {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::StaleParameter => "stale-parameter",
            Self::UndocumentedParameter => "undocumented-parameter",
            Self::Documented => "documented",
        }
    }

    #[must_use]
    pub const fn is_defect(self) -> bool {
        !matches!(self, Self::Documented)
    }
}

/// One definition's documentation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocstringFinding {
    pub issue: DocstringIssue,
    pub name: String,
    pub category: String,
    /// Parameter names the docstring mentions but the lambda list lacks.
    pub stale: Vec<String>,
    /// Parameter names the lambda list has but the docstring never mentions.
    pub undocumented: Vec<String>,
    /// The docstring's first line, so a reader can judge it without opening
    /// the file.
    pub summary: Option<String>,
    pub span: ByteSpan,
}

impl Finding for DocstringFinding {
    fn kind(&self) -> &'static str {
        self.issue.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            self.category.clone(),
            format!("stale=({})", self.stale.join(" ")),
            format!("undocumented=({})", self.undocumented.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("issue", json!(self.issue.label())),
            ("name", json!(self.name)),
            ("category", json!(self.category)),
            ("stale_parameters", json!(self.stale)),
            ("undocumented_parameters", json!(self.undocumented)),
            ("summary", json!(self.summary)),
        ]
    }
}

/// How much of a docstring the finding quotes.
const SUMMARY_LIMIT: usize = 72;

#[must_use]
pub fn build_docstring_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<DocstringFinding> {
    let source = tree.source();
    let mut findings = Vec::new();

    for form in &tree.root_view().children {
        if let Some(finding) = read_definition(form, dialect, source) {
            findings.push(finding);
        }
    }

    let documented = findings
        .iter()
        .filter(|finding: &&DocstringFinding| finding.issue != DocstringIssue::Missing)
        .count();
    let defects = findings
        .iter()
        .filter(|finding| finding.issue.is_defect())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Docstrings are a shape question, not a Common Lisp one: every dialect
        // this build parses puts them in the same place.
        true,
        tree.source(),
        findings,
        vec![
            ("documented_count", json!(documented)),
            ("defect_count", json!(defects)),
        ],
    )
}

fn read_definition(
    form: &ExpressionView,
    dialect: Dialect,
    source: &str,
) -> Option<DocstringFinding> {
    let head = list_head(form)?;
    let shape = definition_shape(dialect, form, head)?;
    // Only definitions that *can* carry a docstring are judged. A `defpackage`
    // or a `defsystem` has no docstring position, so reporting one as missing
    // would be reporting on a language feature that does not exist.
    if !carries_docstring(shape.category) {
        return None;
    }
    let name = shape.name(form)?.to_owned();

    let parameters = shape
        .lambda_parameters(form)
        .map(|parameters| {
            parameters
                .iter()
                .filter_map(atom_symbol_text)
                .filter(|name| !name.starts_with('&'))
                .map(str::to_ascii_uppercase)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let docstring = shape
        .body_forms(form)
        .first()
        .and_then(|first| string_literal(first, source));

    let Some(docstring) = docstring else {
        return Some(DocstringFinding {
            issue: DocstringIssue::Missing,
            name,
            category: shape.category.label().to_owned(),
            stale: Vec::new(),
            undocumented: parameters,
            summary: None,
            span: form.span,
        });
    };

    let mentioned = mentioned_names(&docstring);
    let undocumented = parameters
        .iter()
        .filter(|parameter| !mentioned.iter().any(|name| name == *parameter))
        .cloned()
        .collect::<Vec<_>>();
    // The two directions need different rules, because a mistake costs
    // different things in each.
    //
    // Asking "is this parameter mentioned" is safe for any word: the worst a
    // stray `A` can do is stop `a` being reported as undocumented.
    //
    // Asking "is this word a parameter that no longer exists" is not. Prose is
    // full of `A` and `I`, and a single letter matching nothing would report a
    // stale parameter in a docstring that is correct — sending someone to read
    // documentation that was right. So the stale direction requires more than
    // one character.
    let stale = mentioned
        .iter()
        .filter(|name| name.chars().count() > 1)
        .filter(|name| !parameters.iter().any(|parameter| parameter == *name))
        .cloned()
        .collect::<Vec<_>>();

    let issue = if !stale.is_empty() {
        DocstringIssue::StaleParameter
    } else if !undocumented.is_empty() {
        DocstringIssue::UndocumentedParameter
    } else {
        DocstringIssue::Documented
    };

    Some(DocstringFinding {
        issue,
        name,
        category: shape.category.label().to_owned(),
        stale,
        undocumented,
        summary: Some(elide(&docstring)),
        span: form.span,
    })
}

/// Whether this kind of definition has a docstring position at all.
const fn carries_docstring(category: DefinitionCategory) -> bool {
    matches!(
        category,
        DefinitionCategory::Function
            | DefinitionCategory::Macro
            | DefinitionCategory::GenericFunction
            | DefinitionCategory::Method
            | DefinitionCategory::Variable
            | DefinitionCategory::Constant
            | DefinitionCategory::Parameter
            | DefinitionCategory::Class
            | DefinitionCategory::Struct
            | DefinitionCategory::Condition
    )
}

/// Upper-case words in a docstring, which is how CLHS writes a parameter
/// reference.
///
/// Every such word, single letters included. The caller decides which of them
/// may be treated as evidence in which direction.
fn mentioned_names(docstring: &str) -> Vec<String> {
    let mut names = docstring
        .split(|character: char| !(character.is_alphanumeric() || character == '-'))
        .filter(|word| !word.is_empty())
        .filter(|word| {
            word.chars().any(char::is_alphabetic)
                && word
                    .chars()
                    .filter(|character| character.is_alphabetic())
                    .all(|character| character.is_ascii_uppercase())
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn string_literal(view: &ExpressionView, source: &str) -> Option<String> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = source.get(view.span.start().get()..view.span.end().get())?;
    Some(text.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

fn elide(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= SUMMARY_LIMIT {
        return collapsed;
    }
    let head = collapsed
        .chars()
        .take(SUMMARY_LIMIT.saturating_sub(1))
        .collect::<String>();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<DocstringFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_docstring_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn only(source: &str) -> DocstringFinding {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].clone()
    }

    #[test]
    fn a_definition_with_no_docstring_is_reported() {
        let finding = only("(defun f (x) x)");
        assert_eq!(finding.issue, DocstringIssue::Missing);
        assert_eq!(finding.undocumented, vec!["X".to_owned()]);
    }

    #[test]
    fn a_docstring_naming_every_parameter_is_sound() {
        let finding = only("(defun f (x) \"Returns X unchanged.\" x)");
        assert_eq!(finding.issue, DocstringIssue::Documented);
        assert!(!finding.issue.is_defect());
    }

    #[test]
    fn a_docstring_naming_a_parameter_that_does_not_exist_is_stale() {
        let finding = only("(defun f (limit) \"Stops after COUNT items.\" limit)");
        assert_eq!(finding.issue, DocstringIssue::StaleParameter);
        assert_eq!(finding.stale, vec!["COUNT".to_owned()]);
    }

    #[test]
    fn a_parameter_the_docstring_never_mentions_is_undocumented() {
        let finding = only("(defun f (x y) \"Uses X.\" (list x y))");
        assert_eq!(finding.issue, DocstringIssue::UndocumentedParameter);
        assert_eq!(finding.undocumented, vec!["Y".to_owned()]);
    }

    #[test]
    fn a_stale_name_outranks_an_undocumented_one() {
        // Both are true here; the stale one is the stronger signal.
        let finding = only("(defun f (x y) \"Stops after COUNT items.\" (list x y))");
        assert_eq!(finding.issue, DocstringIssue::StaleParameter);
    }

    #[test]
    fn a_single_letter_word_is_not_read_as_a_parameter_reference() {
        let finding = only("(defun f (x) \"Returns X, a value.\" x)");
        assert!(finding.stale.is_empty(), "{finding:?}");
    }

    #[test]
    fn ordinary_lower_case_prose_is_not_read_as_a_parameter_reference() {
        let finding = only("(defun f (x) \"returns x unchanged\" x)");
        assert!(finding.stale.is_empty(), "{finding:?}");
    }

    #[test]
    fn a_definition_with_no_docstring_position_is_not_judged() {
        let report = report("(defpackage :app (:use :cl))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn the_docstrings_first_line_is_quoted_back() {
        let finding = only("(defun f (x) \"Returns X unchanged.\" x)");
        assert_eq!(finding.summary.as_deref(), Some("Returns X unchanged."));
    }

    #[test]
    fn the_report_answers_for_every_dialect() {
        let tree =
            SyntaxTree::parse_with_dialect("(defn f [x] x)", Dialect::Clojure).expect("parse");
        let report = build_docstring_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(report.dialect_modelled);
    }
}
