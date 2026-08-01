//! Schema-free structural sanity checks for one S-expression data file.
//!
//! Nothing here evaluates the file, and nothing here knows what any format
//! means. Every check reads only the shape a list already commits to by
//! repeating it: a plist alternates key and value, an alist is a run of pair
//! entries, a table of tuples has one arity most of its rows share. Reporting
//! the entry that breaks the pattern needs no schema — the file's own
//! repetition *is* the schema, for exactly the part these checks look at.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list};
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// Which family of checks `data-check` ran, on top of the baseline ones every
/// run performs.
///
/// Only [`DataFormat::Baseline`] exists today. A later phase adds detectors
/// that know a particular data convention; this is the seam they arrive
/// through, so wiring one in will not restructure how the report dispatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    /// No format assumed: duplicate alist/plist keys, odd-length plists, and
    /// mismatched top-level tuple arity. Always runs.
    Baseline,
}

/// What kind of structural inconsistency one finding reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataIssueKind {
    /// The same key appears twice in what looks like an alist or a plist; the
    /// later value silently overrides the earlier one.
    DuplicateKey,
    /// A plist-shaped list ends on a keyword with no value after it.
    OddLengthPlist,
    /// A top-level list of same-shaped tuples has an entry whose arity
    /// disagrees with the rest.
    NestingMismatch,
}

impl DataIssueKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DuplicateKey => "duplicate-key",
            Self::OddLengthPlist => "odd-length-plist",
            Self::NestingMismatch => "nesting-mismatch",
        }
    }
}

/// One structural inconsistency `data-check` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataIssue {
    pub kind: DataIssueKind,
    /// One line of prose naming what triggered the finding: the repeated
    /// key, the dangling keyword, or the arity mismatch.
    pub detail: String,
    pub span: ByteSpan,
}

impl Finding for DataIssue {
    fn kind(&self) -> &'static str {
        self.kind.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.detail.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("detail", json!(self.detail))]
    }
}

/// Builds one file's `data-check` report.
#[must_use]
pub fn build_data_check_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    format: DataFormat,
) -> FileFindings<DataIssue> {
    let mut findings = Vec::new();
    match format {
        DataFormat::Baseline => {
            let root = tree.root_view();
            for_each_subview(&root, |view| {
                findings.extend(key_shape_issues(view));
            });
            for top_level in &root.children {
                findings.extend(nesting_mismatch_issues(top_level));
            }
        }
    }

    // Every check above reads only the balanced-parens shape, with no
    // operator vocabulary consulted, so it means the same thing for every
    // dialect this tool parses — there is no dialect this report has nothing
    // to say about.
    let modelled = true;

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        Vec::new(),
    )
}

/// Duplicate-key and odd-length-plist findings for one list node, if `view`
/// itself is shaped like a plist or an alist.
///
/// A node is checked as at most one of the two shapes: a plist's own children
/// alternate key and value directly, while an alist's children are
/// themselves pair entries. Those shapes cannot both hold for the same list
/// at once, so trying the plist reading first and falling back to the alist
/// reading cannot double-report one list.
fn key_shape_issues(view: &ExpressionView) -> Vec<DataIssue> {
    if !is_paren_list(view) || view.children.len() < 2 {
        return Vec::new();
    }

    match plist_issues(view) {
        Some(issues) => issues,
        None => alist_issues(view),
    }
}

/// The keyword text of an atom in key position, or `None` when it is not one.
///
/// Restricted to keyword atoms (`:foo`) rather than any bare symbol: a flat
/// list of alternating symbols and values is common in code that has nothing
/// to do with a plist (`(a 1 b 2)` inside an arbitrary literal), while a
/// keyword in every key position is the one shape ordinary Lisp code does
/// not produce by accident.
fn keyword_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view).filter(|text| text.starts_with(':'))
}

/// Reads `view`'s children as a plist — key, value, key, value, … — when
/// every key position holds a keyword atom. Returns `None` when that does not
/// hold everywhere, so the caller can try the alist shape instead rather than
/// treating a partial match as a plist with holes.
fn plist_issues(view: &ExpressionView) -> Option<Vec<DataIssue>> {
    let children = &view.children;
    let key_slots = children.len().div_ceil(2);
    let every_key_slot_is_a_keyword =
        (0..key_slots).all(|slot| keyword_text(&children[slot * 2]).is_some());
    if !every_key_slot_is_a_keyword {
        return None;
    }

    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < children.len() {
        let key_view = &children[index];
        let key = keyword_text(key_view).expect("key slot holds a keyword, checked above");

        if children.get(index + 1).is_none() {
            issues.push(DataIssue {
                kind: DataIssueKind::OddLengthPlist,
                detail: format!("{key} has no value; the plist ends here"),
                span: key_view.span,
            });
            break;
        }

        if !seen.insert(key) {
            issues.push(DataIssue {
                kind: DataIssueKind::DuplicateKey,
                detail: format!(
                    "{key} repeats; the later value silently overrides the earlier one"
                ),
                span: key_view.span,
            });
        }
        index += 2;
    }
    Some(issues)
}

/// Reads `view`'s children as an alist — a run of `(key . value)` or
/// `(key value)` entries — and reports a key that repeats across entries.
///
/// Every child must resolve to an entry key or nothing is reported: one
/// child that is not shaped like an entry (a bare atom, a pair whose key is
/// not an atom, a list of some other arity) means this node is not
/// confidently an alist, and guessing which of its children to check anyway
/// would trade a real signal for a noisy one.
fn alist_issues(view: &ExpressionView) -> Vec<DataIssue> {
    let mut keys = Vec::with_capacity(view.children.len());
    for child in &view.children {
        match alist_entry_key(child) {
            Some(key_view) => keys.push(key_view),
            None => return Vec::new(),
        }
    }

    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    for key_view in keys {
        let Some(key) = atom_text(key_view) else {
            continue;
        };
        if !seen.insert(key) {
            issues.push(DataIssue {
                kind: DataIssueKind::DuplicateKey,
                detail: format!(
                    "{key} repeats; the later value silently overrides the earlier one"
                ),
                span: key_view.span,
            });
        }
    }
    issues
}

/// The key atom of one alist entry — `(key . value)` or `(key value)` — or
/// `None` when `child` is not shaped like an entry.
fn alist_entry_key(child: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(child) {
        return None;
    }
    let key = child.children.first()?;
    atom_text(key)?;
    match child.children.len() {
        2 => Some(key),
        3 if is_dot(&child.children[1]) => Some(key),
        _ => None,
    }
}

/// Whether `view` is the `.` of a dotted pair — punctuation, not a value.
///
/// This tool's structural parser has no notion of a cons cell: `(a . b)`
/// parses as a three-child list whose middle child is a plain atom spelled
/// `.`. Recognising that atom is the only way to read a dotted-pair alist
/// entry at all.
fn is_dot(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view.reader_prefixes.is_empty()
        && atom_text(view) == Some(".")
}

/// Reports the entries of `top_level` whose arity disagrees with the shape
/// most of its sibling entries share.
///
/// Deliberately conservative: this fires only when `top_level` is itself a
/// paren list of three or more paren-list children, and only when a strict
/// majority of them share one arity. A file mixing genuinely different
/// per-entry shapes — the common case for anything that is not literally a
/// table of same-shaped rows — has no majority and is left alone entirely,
/// which is the point: a false "your data is malformed" is worse than
/// staying quiet on a real one.
fn nesting_mismatch_issues(top_level: &ExpressionView) -> Vec<DataIssue> {
    if !is_paren_list(top_level) || top_level.children.len() < 3 {
        return Vec::new();
    }
    if !top_level.children.iter().all(is_paren_list) {
        return Vec::new();
    }

    let mut arity_counts: HashMap<usize, usize> = HashMap::new();
    for entry in &top_level.children {
        *arity_counts.entry(entry.children.len()).or_insert(0) += 1;
    }

    let Some((&majority_arity, &majority_count)) =
        arity_counts.iter().max_by_key(|(_, count)| **count)
    else {
        return Vec::new();
    };
    // A strict majority is unique when it exists, so which candidate
    // `max_by_key` happened to visit last cannot change this outcome.
    if majority_count * 2 <= top_level.children.len() {
        return Vec::new();
    }

    top_level
        .children
        .iter()
        .filter(|entry| entry.children.len() != majority_arity)
        .map(|entry| DataIssue {
            kind: DataIssueKind::NestingMismatch,
            detail: format!(
                "has {} element(s); {majority_count} of {} sibling entries have {majority_arity}",
                entry.children.len(),
                top_level.children.len()
            ),
            span: entry.span,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<DataIssue> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_data_check_report(
            Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            DataFormat::Baseline,
        )
    }

    #[test]
    fn a_dotted_alist_with_a_genuine_duplicate_key_is_reported() {
        let report = report("((key1 . v1) (key2 . v2) (key1 . v3))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
        assert!(report.findings[0].detail.contains("key1"), "{report:?}");
    }

    #[test]
    fn a_proper_list_alist_with_a_genuine_duplicate_key_is_reported() {
        let report = report("((key1 v1) (key2 v2) (key1 v3))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
    }

    #[test]
    fn an_alist_with_no_duplicates_reports_nothing() {
        let report = report("((key1 . v1) (key2 . v2) (key3 . v3))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_well_formed_plist_reports_nothing() {
        let report = report("(:a 1 :b 2 :c 3)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_plist_with_a_duplicate_key_is_reported() {
        let report = report("(:a 1 :b 2 :a 3)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
        assert!(report.findings[0].detail.contains(":a"), "{report:?}");
    }

    #[test]
    fn a_plist_with_an_odd_trailing_keyword_is_reported() {
        let report = report("(:a 1 :b)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::OddLengthPlist);
        assert!(report.findings[0].detail.contains(":b"), "{report:?}");
    }

    #[test]
    fn a_file_with_no_data_like_structure_reports_nothing_and_does_not_crash() {
        let report = report("(defun add (a b) (+ a b))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_atom_only_file_reports_nothing_and_does_not_crash() {
        let report = report("42");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_empty_file_reports_nothing_and_does_not_crash() {
        let report = report("");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_top_level_tuple_table_with_one_mismatched_entry_is_reported() {
        let report = report("((a 1 2) (b 1 2) (c 1 2) (d 1))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::NestingMismatch);
    }

    #[test]
    fn a_top_level_tuple_table_with_no_clear_majority_reports_nothing() {
        // Three entries, three different arities: no strict majority, so this
        // stays quiet rather than guessing which entry is "wrong".
        let report = report("((a 1) (b 1 2) (c 1 2 3))");
        assert!(
            report
                .findings
                .iter()
                .all(|issue| issue.kind != DataIssueKind::NestingMismatch),
            "{report:?}"
        );
    }

    #[test]
    fn dialect_modelled_is_always_true() {
        let tree = SyntaxTree::parse_with_dialect("(:a 1)", Dialect::Fennel).expect("parse");
        let report = build_data_check_report(
            Path::new("t.fnl"),
            Dialect::Fennel,
            &tree,
            DataFormat::Baseline,
        );
        assert!(report.dialect_modelled);
    }
}
