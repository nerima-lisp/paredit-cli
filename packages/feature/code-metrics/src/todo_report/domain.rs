//! `TODO`, `FIXME`, `XXX`, and `HACK` markers, with the definition each sits in.
//!
//! Task markers are the one form of project state that lives in the source and
//! nowhere else. They are also, structurally, invisible: comments are kept as
//! trivia beside the tree rather than as nodes in it, so no report that walks
//! forms can see one.
//!
//! Reading them as *structure* rather than as `grep` output is what this adds.
//! A marker's line number is not much use on its own; the definition it sits
//! inside is, because that is the unit of work someone would pick up. So is the
//! marker's own text, separated from the `;;` and the marker word, and the
//! author when the comment carries a `TODO(name):` attribution.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// The markers this report recognises, in descending urgency.
///
/// Ordered rather than alphabetical: a comment saying `FIXME` and `TODO` in one
/// breath is a `FIXME`, and a consumer sorting by marker wants the urgent ones
/// first.
const MARKERS: [&str; 5] = ["FIXME", "XXX", "BUG", "HACK", "TODO"];

/// One task marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMarker {
    /// The marker word, upper-cased.
    pub marker: String,
    /// The comment text after the marker and any attribution.
    pub note: String,
    /// The name in a `TODO(name):` attribution, when there is one.
    pub author: Option<String>,
    /// The top-level definition the comment sits inside, when it sits in one.
    /// The unit of work a reader would actually pick up.
    pub definition: Option<String>,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for TaskMarker {
    fn kind(&self) -> &'static str {
        // A `&'static str` is required, so the marker is matched back onto the
        // table rather than leaked from the owned string.
        MARKERS
            .iter()
            .find(|known| **known == self.marker)
            .copied()
            .unwrap_or("TODO")
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.definition.clone().unwrap_or_else(|| "-".to_owned()),
            self.author.clone().unwrap_or_else(|| "-".to_owned()),
            self.note.clone(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("marker", json!(self.marker)),
            ("note", json!(self.note)),
            ("author", json!(self.author)),
            ("definition", json!(self.definition)),
        ]
    }
}

#[must_use]
pub fn build_todo_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<TaskMarker> {
    let source = tree.source();
    let definitions = top_level_definitions(tree, dialect);

    let findings = tree
        .comments()
        .filter_map(|comment| {
            let span = comment.span();
            let text = source.get(span.start().get()..span.end().get())?;
            let (marker, rest) = split_marker(text)?;
            let (author, note) = split_author(rest);
            Some(TaskMarker {
                marker: marker.to_owned(),
                note,
                author,
                definition: enclosing_definition(&definitions, span),
                span,
                line: line_of(source, span.start().get()),
            })
        })
        .collect::<Vec<_>>();

    let urgent = findings
        .iter()
        .filter(|marker: &&TaskMarker| matches!(marker.marker.as_str(), "FIXME" | "XXX" | "BUG"))
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Comment syntax is `;` in every dialect this tool parses, so the
        // analysis is complete for all of them.
        true,
        findings,
        vec![("urgent_count", json!(urgent))],
    )
}

/// The name and extent of each top-level definition, in source order.
fn top_level_definitions(tree: &SyntaxTree, dialect: Dialect) -> Vec<(String, ByteSpan)> {
    tree.root_view()
        .children
        .iter()
        .filter_map(|form| {
            let head = list_head(form)?;
            let shape = definition_shape(dialect, form, head)?;
            Some((shape.name(form)?.to_owned(), form.span))
        })
        .collect()
}

fn enclosing_definition(definitions: &[(String, ByteSpan)], span: ByteSpan) -> Option<String> {
    definitions
        .iter()
        .find(|(_, extent)| {
            extent.start().get() <= span.start().get() && span.end().get() <= extent.end().get()
        })
        .map(|(name, _)| name.clone())
}

/// Splits a comment into its marker word and the rest.
///
/// The marker must be a whole word: `TODOs` in prose is not a task, and
/// `AUTOLOAD` is not a `LOAD`. Matching is case-insensitive because both
/// `TODO` and `todo` are written in practice.
fn split_marker(comment: &str) -> Option<(&'static str, &str)> {
    let body = comment.trim_start_matches(';').trim_start();
    for marker in MARKERS {
        let Some(rest) = body.get(..marker.len()) else {
            continue;
        };
        if !rest.eq_ignore_ascii_case(marker) {
            continue;
        }
        let tail = &body[marker.len()..];
        // A word boundary: end of comment, punctuation, or whitespace.
        if tail
            .chars()
            .next()
            .is_none_or(|character| !character.is_alphanumeric() && character != '-')
        {
            return Some((marker, tail));
        }
    }
    None
}

/// Splits `(name): note` into its attribution and its note.
fn split_author(rest: &str) -> (Option<String>, String) {
    let trimmed = rest.trim_start();
    if let Some(inner) = trimmed.strip_prefix('(') {
        if let Some((author, tail)) = inner.split_once(')') {
            return (Some(author.trim().to_owned()), clean_note(tail));
        }
    }
    (None, clean_note(trimmed))
}

fn clean_note(text: &str) -> String {
    text.trim_start()
        .trim_start_matches(':')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<TaskMarker> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_todo_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn only(source: &str) -> TaskMarker {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].clone()
    }

    #[test]
    fn a_todo_comment_is_reported_with_its_note() {
        let marker = only("(defun f () 1) ; TODO: handle the empty case");
        assert_eq!(marker.marker, "TODO");
        assert_eq!(marker.note, "handle the empty case");
    }

    #[test]
    fn an_attribution_is_split_from_the_note() {
        let marker = only("; TODO(ada): rewrite this");
        assert_eq!(marker.author.as_deref(), Some("ada"));
        assert_eq!(marker.note, "rewrite this");
    }

    #[test]
    fn every_marker_word_is_recognised() {
        for word in ["TODO", "FIXME", "XXX", "HACK", "BUG"] {
            let marker = only(&format!("; {word} something"));
            assert_eq!(marker.marker, word);
        }
    }

    #[test]
    fn a_lower_case_marker_is_recognised_too() {
        assert_eq!(only("; todo: later").marker, "TODO");
    }

    #[test]
    fn a_marker_inside_a_longer_word_is_not_a_task() {
        let report = report("; TODOs are tracked elsewhere");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_ordinary_comment_is_not_a_task() {
        let report = report("; this explains the next form");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_marker_inside_a_definition_names_the_definition() {
        let marker = only("(defun render (x)\n  ; TODO: cache this\n  x)");
        assert_eq!(marker.definition.as_deref(), Some("render"));
    }

    #[test]
    fn a_marker_outside_every_definition_names_none() {
        let marker = only("; TODO: split this file\n(defun f () 1)");
        assert_eq!(marker.definition, None);
    }

    #[test]
    fn the_urgent_markers_are_counted_separately() {
        let report = report("; TODO: later\n; FIXME: now\n; XXX: also now");
        assert_eq!(report.summary, vec![("urgent_count", json!(2))]);
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("; TODO: a\n; TODO: b\n; TODO: c");
        let starts = report
            .findings
            .iter()
            .map(|marker| marker.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }
}
