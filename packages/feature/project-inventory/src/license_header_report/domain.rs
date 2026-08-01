//! Whether each file opens with a license header, and per file.
//!
//! A license notice that lives in `LICENSE` at the repository root says
//! nothing about any one file: a file copied out of the tree, vendored
//! elsewhere, or read by a tool that only ever sees one file at a time
//! carries no notice unless the file itself does. Whether it does is not
//! visible to a tree walk either — the header is a leading run of comments,
//! not a node the parser gives a shape to, so a report that only walks
//! `children` cannot see one exists.
//!
//! Two separate questions are answered:
//!
//! - **Presence** — does the file start with a comment block that looks like
//!   a license header at all (case-insensitively mentioning "license",
//!   "copyright", or an SPDX identifier)? Answered per file, here.
//! - **Consistency** — for files that do have one, does its text match the
//!   header most files in the analyzed set carry? A header that was
//!   copy-pasted and then edited, or one left behind by a stale template, is
//!   invisible to the presence check alone. Answering this needs every
//!   file's header at once, so it is answered separately, in
//!   [`crate::license_header_report::usecase::apply_header_consistency`],
//!   after every file has been read.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_cli::shared::bounded_preview;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use serde_json::{Value, json};

/// The excerpt cap for `json_fields`'s header preview, matching the cap
/// `constant_report` uses for a literal's own preview.
const SNIPPET_LIMIT: usize = 64;

/// Case-insensitive words that make a leading comment block a license
/// header rather than an ordinary file banner.
///
/// `"license"` alone already matches inside `"spdx-license-identifier"`, but
/// the SPDX form is listed anyway: it is the marker most tooling actually
/// looks for, and a reader of this table should not have to work that out.
const LICENSE_KEYWORDS: [&str; 3] = ["license", "copyright", "spdx-license-identifier"];

/// What one file's header amounts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderStatus {
    /// No leading comment block, or one that does not read as a license
    /// notice.
    Missing,
    /// A license-shaped header, not yet compared against the rest of the
    /// fileset.
    Present,
    /// A license-shaped header whose text differs from the majority header
    /// in the analyzed fileset. Set only by
    /// [`crate::license_header_report::usecase::apply_header_consistency`],
    /// never by [`build_license_header_report`], since no single file's
    /// analysis can know what the majority is.
    Inconsistent,
}

impl HeaderStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Present => "present",
            Self::Inconsistent => "inconsistent",
        }
    }
}

/// One file's license-header finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseHeaderItem {
    pub status: HeaderStatus,
    /// The header's exact text, joined from its comment lines. `None` when
    /// [`HeaderStatus::Missing`], since there is nothing to show.
    pub header_text: Option<String>,
    pub span: ByteSpan,
}

impl Finding for LicenseHeaderItem {
    fn kind(&self) -> &'static str {
        self.status.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.status.label().to_owned(),
            self.header_text.as_deref().map_or_else(
                || "-".to_owned(),
                |text| bounded_preview(text, SNIPPET_LIMIT),
            ),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("status", json!(self.status.label())),
            (
                "header_excerpt",
                json!(
                    self.header_text
                        .as_deref()
                        .map(|text| bounded_preview(text, SNIPPET_LIMIT))
                ),
            ),
        ]
    }
}

#[must_use]
pub fn build_license_header_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<LicenseHeaderItem> {
    let source = tree.source();
    let block = leading_comment_block(tree, source);

    let item = match block {
        Some((span, text)) if looks_like_license_header(&text) => LicenseHeaderItem {
            status: HeaderStatus::Present,
            header_text: Some(text),
            span,
        },
        _ => LicenseHeaderItem {
            status: HeaderStatus::Missing,
            header_text: None,
            span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0)),
        },
    };

    let missing = usize::from(item.status == HeaderStatus::Missing);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Comment syntax varies in its spelling (`;` almost everywhere,
        // `#` in Janet) but every dialect this tool parses has one, and
        // `SyntaxTree::comments()` already normalizes the spelling away, so
        // the analysis is complete for all of them.
        true,
        source,
        vec![item],
        vec![
            ("missing_count", json!(missing)),
            // Filled in by `apply_header_consistency` once the whole
            // fileset's headers are known; `0` here is provisional, not a
            // claim that this file's header is consistent.
            ("inconsistent_count", json!(0)),
        ],
    )
}

/// The file's leading run of comments, if it has one, as a single joined
/// text together with the span it occupies.
///
/// "Leading" is read strictly: only whitespace may precede the first
/// comment, and only whitespace may separate each comment from the next one
/// still counted. A comment reached after the first non-comment content —
/// even a blank line followed by code before any comment — is not part of
/// the header; a docstring or a trailing comment on the first form must not
/// be mistaken for one.
fn leading_comment_block(tree: &SyntaxTree, source: &str) -> Option<(ByteSpan, String)> {
    let mut comments = tree.comments();
    let first = comments.next()?;
    if !source.get(..first.span().start().get())?.trim().is_empty() {
        return None;
    }

    let mut end = first.span().end();
    let mut lines = vec![first.text().trim_end().to_owned()];
    for comment in comments {
        let between = source.get(end.get()..comment.span().start().get())?;
        if !between.trim().is_empty() {
            break;
        }
        lines.push(comment.text().trim_end().to_owned());
        end = comment.span().end();
    }

    Some((
        ByteSpan::new(first.span().start(), end),
        lines.join("\n").trim().to_owned(),
    ))
}

/// Whether a leading comment block reads as a license notice rather than an
/// ordinary file banner.
fn looks_like_license_header(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    LICENSE_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<LicenseHeaderItem> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_license_header_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn only(source: &str) -> LicenseHeaderItem {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].clone()
    }

    #[test]
    fn a_file_with_no_comments_at_all_is_missing_a_header() {
        let item = only("(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Missing);
        assert_eq!(item.header_text, None);
    }

    #[test]
    fn a_leading_comment_with_no_license_words_is_missing_a_header() {
        let item = only(";; a helper for rendering panes\n(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Missing);
    }

    #[test]
    fn a_leading_copyright_comment_is_a_present_header() {
        let item = only(";; Copyright 2024 Example Corp\n(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Present);
        assert_eq!(
            item.header_text.as_deref(),
            Some(";; Copyright 2024 Example Corp")
        );
    }

    #[test]
    fn an_spdx_identifier_is_recognised_case_insensitively() {
        let item = only(";; spdx-license-identifier: MIT\n(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Present);
    }

    #[test]
    fn the_word_license_is_recognised_regardless_of_case() {
        let item = only(";; LICENSE: MIT\n(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Present);
    }

    #[test]
    fn several_leading_comment_lines_are_joined_into_one_header() {
        let item = only(";; Copyright 2024\n;; Licensed under MIT\n(defun f () 1)");
        assert_eq!(
            item.header_text.as_deref(),
            Some(";; Copyright 2024\n;; Licensed under MIT")
        );
    }

    #[test]
    fn a_comment_that_only_trails_code_is_not_a_leading_header() {
        let item = only("(defun f () 1) ;; Copyright 2024");
        assert_eq!(item.status, HeaderStatus::Missing);
    }

    #[test]
    fn a_blank_line_inside_the_header_does_not_break_it() {
        let item = only(";; Copyright 2024\n\n;; Licensed under MIT\n(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Present);
        assert!(item.header_text.as_deref().unwrap().contains("MIT"));
    }

    #[test]
    fn code_between_two_comment_runs_stops_the_header_at_the_first_run() {
        let item = only(";; a helper\n(defvar x 1)\n;; Copyright 2024\n(defun f () 1)");
        assert_eq!(item.status, HeaderStatus::Missing);
    }

    #[test]
    fn the_missing_count_reflects_the_files_own_status() {
        let report = report("(defun f () 1)");
        assert_eq!(report.summary[0], ("missing_count", json!(1)));
    }

    #[test]
    fn the_inconsistent_count_starts_at_zero_pending_the_fileset_pass() {
        let report = report(";; Copyright 2024\n(defun f () 1)");
        assert_eq!(report.summary[1], ("inconsistent_count", json!(0)));
    }

    #[test]
    fn a_janet_hash_comment_header_is_recognised_too() {
        let tree = SyntaxTree::parse_with_dialect(
            "# Copyright 2024 Example Corp\n(defn f [] 1)",
            Dialect::Janet,
        )
        .expect("parse");
        let report = build_license_header_report(Path::new("t.janet"), Dialect::Janet, &tree);
        assert_eq!(report.findings[0].status, HeaderStatus::Present);
        assert!(report.dialect_modelled);
    }
}
