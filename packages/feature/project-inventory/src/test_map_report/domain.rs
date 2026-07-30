//! Which definitions have a test, and which do not.
//!
//! A list of tests is easy to get. A list of definitions is easy to get.
//! Neither answers the question anyone actually has, which is the *pairing* —
//! and the pairing is what this report is.
//!
//! Tests are matched to subjects by name convention, because that is the only
//! link that exists: nothing in `deftest render-pane` says it tests
//! `render-pane` except the name. The conventions recognised are the ones in
//! use — an exact name, a `test-` or `-test` affix, and `%-tests` — and a test
//! whose name matches none of them is reported as *unattributed* rather than
//! silently dropped, because that is a real finding: a test nobody can tell
//! what it covers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// What one entry in the map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// A definition with at least one test naming it.
    Tested,
    /// A definition no test names.
    Untested,
    /// A test whose name matches no definition in the analyzed files.
    Unattributed,
}

impl Coverage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tested => "tested",
            Self::Untested => "untested",
            Self::Unattributed => "unattributed",
        }
    }
}

/// One definition, or one test with no subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageEntry {
    pub coverage: Coverage,
    pub name: String,
    /// The tests that name this definition.
    pub tests: Vec<String>,
    pub span: ByteSpan,
}

impl Finding for CoverageEntry {
    fn kind(&self) -> &'static str {
        self.coverage.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            format!("tests=({})", self.tests.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("coverage", json!(self.coverage.label())),
            ("name", json!(self.name)),
            ("tests", json!(self.tests)),
        ]
    }
}

#[must_use]
pub fn build_test_map_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<CoverageEntry> {
    let mut subjects: Vec<(String, ByteSpan)> = Vec::new();
    let mut tests: Vec<(String, ByteSpan)> = Vec::new();

    for form in &tree.root_view().children {
        let Some(head) = list_head(form) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, form, head) else {
            continue;
        };
        let Some(name) = shape.name(form) else {
            continue;
        };
        if shape.category == DefinitionCategory::Test {
            tests.push((fold(name), form.span));
        } else if is_testable(shape.category) {
            subjects.push((fold(name), form.span));
        }
    }

    let subject_names: BTreeSet<&String> = subjects.iter().map(|(name, _)| name).collect();
    let mut by_subject: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unattributed = Vec::new();
    for (test, span) in &tests {
        match candidates(test)
            .into_iter()
            .find(|candidate| subject_names.contains(candidate))
        {
            Some(subject) => by_subject.entry(subject).or_default().push(test.clone()),
            None => unattributed.push((test.clone(), *span)),
        }
    }

    let mut findings = subjects
        .iter()
        .map(|(name, span)| {
            let tests = by_subject.get(name).cloned().unwrap_or_default();
            CoverageEntry {
                coverage: if tests.is_empty() {
                    Coverage::Untested
                } else {
                    Coverage::Tested
                },
                name: name.clone(),
                tests,
                span: *span,
            }
        })
        .collect::<Vec<_>>();
    findings.extend(unattributed.into_iter().map(|(name, span)| CoverageEntry {
        coverage: Coverage::Unattributed,
        name,
        tests: Vec::new(),
        span,
    }));

    let tested = findings
        .iter()
        .filter(|entry| entry.coverage == Coverage::Tested)
        .count();
    let testable = subjects.len();
    // Per mille, so a CI gate compares integers rather than float formatting.
    // A file with nothing testable is fully covered rather than a total miss.
    let coverage_per_mille = tested
        .saturating_mul(1000)
        .checked_div(testable)
        .unwrap_or(1000);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Definition shapes and naming conventions are dialect-neutral.
        true,
        tree.source(),
        findings,
        vec![
            ("testable_count", json!(testable)),
            ("test_count", json!(tests.len())),
            ("coverage_per_mille", json!(coverage_per_mille)),
        ],
    )
}

/// The definition names a test name could plausibly be about.
///
/// Ordered most specific first, so `test-render-pane` attributes to
/// `render-pane` rather than to a definition that happens to be called
/// `test-render-pane`.
fn candidates(test: &str) -> Vec<String> {
    let mut found = Vec::new();
    for prefix in ["TEST-", "%"] {
        if let Some(rest) = test.strip_prefix(prefix) {
            if !rest.is_empty() {
                found.push(rest.to_owned());
            }
        }
    }
    // `-TESTS` before `-TEST`, so `render-tests` strips the whole suffix
    // rather than leaving a trailing `S`.
    for suffix in ["-TESTS", "-TEST"] {
        if let Some(rest) = test.strip_suffix(suffix) {
            if !rest.is_empty() {
                found.push(rest.to_owned());
            }
        }
    }
    // An exact match last: a `deftest render-pane` beside a `defun render-pane`
    // is the least ambiguous convention but the least common one, so the
    // affix forms get first refusal.
    found.push(test.to_owned());
    found
}

/// Whether a definition is the kind of thing a test covers.
///
/// A package or a system is not; asserting that `defpackage app` is untested
/// would make the coverage number meaningless.
const fn is_testable(category: DefinitionCategory) -> bool {
    matches!(
        category,
        DefinitionCategory::Function
            | DefinitionCategory::Macro
            | DefinitionCategory::GenericFunction
            | DefinitionCategory::Method
    )
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<CoverageEntry> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_test_map_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn entry<'a>(report: &'a FileFindings<CoverageEntry>, name: &str) -> &'a CoverageEntry {
        report
            .findings
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is reported: {report:?}"))
    }

    #[test]
    fn a_test_prefixed_name_attributes_to_its_subject() {
        let report = report("(defun render (x) x)\n(deftest test-render () 1)");
        assert_eq!(entry(&report, "render").coverage, Coverage::Tested);
        assert_eq!(
            entry(&report, "render").tests,
            vec!["TEST-RENDER".to_owned()]
        );
    }

    #[test]
    fn a_test_suffixed_name_attributes_too() {
        let report = report("(defun render (x) x)\n(deftest render-test () 1)");
        assert_eq!(entry(&report, "render").coverage, Coverage::Tested);
    }

    #[test]
    fn a_definition_no_test_names_is_untested() {
        let report = report("(defun render (x) x)");
        assert_eq!(entry(&report, "render").coverage, Coverage::Untested);
    }

    #[test]
    fn a_test_matching_no_definition_is_reported_rather_than_dropped() {
        let report = report("(defun render (x) x)\n(deftest test-missing () 1)");
        assert_eq!(
            entry(&report, "test-missing").coverage,
            Coverage::Unattributed
        );
    }

    #[test]
    fn a_package_is_not_something_a_test_covers() {
        let report = report("(defpackage :app (:use :cl))\n(defun render (x) x)");
        assert_eq!(report.summary[0], ("testable_count", json!(1)));
    }

    #[test]
    fn coverage_is_tested_over_testable() {
        let report = report("(defun a () 1)\n(defun b () 2)\n(deftest test-a () 1)");
        assert_eq!(report.summary[2], ("coverage_per_mille", json!(500)));
    }

    #[test]
    fn a_file_with_nothing_testable_is_not_reported_as_a_total_miss() {
        let report = report("(defpackage :app (:use :cl))");
        assert_eq!(report.summary[2], ("coverage_per_mille", json!(1000)));
    }

    #[test]
    fn two_tests_for_one_definition_are_both_attributed() {
        let report = report("(defun a () 1)\n(deftest test-a () 1)\n(deftest a-test () 2)");
        assert_eq!(entry(&report, "a").tests.len(), 2);
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun a () 1)\n(defun b () 2)\n(defun c () 3)");
        let starts = report
            .findings
            .iter()
            .map(|entry| entry.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }
}
