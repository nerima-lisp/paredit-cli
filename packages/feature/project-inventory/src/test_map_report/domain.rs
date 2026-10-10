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

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_semantics::semantics::project::service::{resolve_file_packages, resolve_symbol};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
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
    build_test_map_reports(&[(path, dialect, tree)]).remove(0)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SubjectIdentity {
    emacs_lisp: bool,
    package: Option<String>,
    local_file: Option<usize>,
    name: String,
}

struct NamedForm {
    name: String,
    identity: SubjectIdentity,
    span: ByteSpan,
}

fn collect_forms(
    file_index: usize,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> (Vec<NamedForm>, Vec<NamedForm>) {
    let packages = resolve_file_packages(dialect, tree);
    let mut subjects = Vec::new();
    let mut tests = Vec::new();
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
        if shape.category != DefinitionCategory::Test && !is_testable(shape.category) {
            continue;
        }
        let symbol = (dialect == Dialect::CommonLisp)
            .then(|| resolve_symbol(&packages, name, form.span.start().get()))
            .flatten();
        let identity = SubjectIdentity {
            emacs_lisp: dialect == Dialect::EmacsLisp,
            package: symbol
                .as_ref()
                .map(|symbol| symbol.package().as_str().to_owned()),
            // An unknown reader package is comparable only within one file.
            local_file: (dialect == Dialect::CommonLisp && symbol.is_none()).then_some(file_index),
            name: symbol.as_ref().map_or_else(
                || fold(name, dialect),
                |symbol| symbol.name().as_str().to_owned(),
            ),
        };
        let named = NamedForm {
            name: fold(name, dialect),
            identity,
            span: form.span,
        };
        if shape.category == DefinitionCategory::Test {
            tests.push(named);
        } else {
            subjects.push(named);
        }
    }
    (subjects, tests)
}

#[must_use]
pub fn build_test_map_reports(
    files: &[(&Path, Dialect, &SyntaxTree)],
) -> Vec<FileFindings<CoverageEntry>> {
    let forms = files
        .iter()
        .enumerate()
        .map(|(index, (_, dialect, tree))| collect_forms(index, *dialect, tree))
        .collect::<Vec<_>>();
    let mut subjects_by_identity: BTreeMap<SubjectIdentity, Vec<(usize, usize)>> = BTreeMap::new();
    for (file, (subjects, _)) in forms.iter().enumerate() {
        for (subject, form) in subjects.iter().enumerate() {
            subjects_by_identity
                .entry(form.identity.clone())
                .or_default()
                .push((file, subject));
        }
    }
    let mut by_subject: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    let mut unattributed = vec![Vec::new(); files.len()];
    for (file, (_, tests)) in forms.iter().enumerate() {
        for test in tests {
            let matched = candidates(&test.identity.name, files[file].1)
                .into_iter()
                .find_map(|name| {
                    let identity = SubjectIdentity {
                        name,
                        ..test.identity.clone()
                    };
                    subjects_by_identity.get(&identity)
                });
            // Duplicate subjects are not evidence of coverage for either definition.
            if let Some(subjects) = matched.filter(|subjects| subjects.len() == 1) {
                by_subject
                    .entry(subjects[0])
                    .or_default()
                    .push(test.name.clone());
            } else {
                unattributed[file].push((test.name.clone(), test.span));
            }
        }
    }
    files
        .iter()
        .enumerate()
        .map(|(file, (path, dialect, tree))| {
            let (subjects, tests) = &forms[file];
            let mut findings = subjects
                .iter()
                .enumerate()
                .map(|(index, form)| {
                    let tests = by_subject.get(&(file, index)).cloned().unwrap_or_default();
                    CoverageEntry {
                        coverage: if tests.is_empty() {
                            Coverage::Untested
                        } else {
                            Coverage::Tested
                        },
                        name: form.name.clone(),
                        tests,
                        span: form.span,
                    }
                })
                .collect::<Vec<_>>();
            findings.extend(unattributed[file].iter().map(|(name, span)| CoverageEntry {
                coverage: Coverage::Unattributed,
                name: name.clone(),
                tests: Vec::new(),
                span: *span,
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
                *dialect,
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
        })
        .collect()
}

/// The definition names a test name could plausibly be about.
///
/// Ordered most specific first, so `test-render-pane` attributes to
/// `render-pane` rather than to a definition that happens to be called
/// `test-render-pane`.
fn candidates(test: &str, dialect: Dialect) -> Vec<String> {
    let mut found = Vec::new();
    let prefixes = if dialect == Dialect::EmacsLisp {
        ["test-", "%"]
    } else {
        ["TEST-", "%"]
    };
    for prefix in prefixes {
        if let Some(rest) = test.strip_prefix(prefix) {
            if !rest.is_empty() {
                found.push(rest.to_owned());
            }
        }
    }
    // `-TESTS` before `-TEST`, so `render-tests` strips the whole suffix
    // rather than leaving a trailing `S`.
    let suffixes = if dialect == Dialect::EmacsLisp {
        ["-tests", "-test"]
    } else {
        ["-TESTS", "-TEST"]
    };
    for suffix in suffixes {
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

fn fold(name: &str, dialect: Dialect) -> String {
    if dialect == Dialect::CommonLisp {
        common_lisp_symbol_reference_needle(name)
    } else {
        name.to_owned()
    }
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

    fn reports(sources: &[(&str, Dialect)]) -> Vec<FileFindings<CoverageEntry>> {
        let trees = sources
            .iter()
            .map(|(source, dialect)| {
                SyntaxTree::parse_with_dialect(source, *dialect).expect("parse")
            })
            .collect::<Vec<_>>();
        let files = trees
            .iter()
            .zip(sources)
            .map(|(tree, (_, dialect))| (Path::new("test.lisp"), *dialect, tree))
            .collect::<Vec<_>>();
        build_test_map_reports(&files)
    }

    #[test]
    fn separate_test_file_covers_a_definition_in_the_same_package() {
        let reports = reports(&[
            ("(in-package :app) (defun render () 1)", Dialect::CommonLisp),
            (
                "(in-package #:app) (deftest test-render () 1)",
                Dialect::CommonLisp,
            ),
        ]);
        assert_eq!(entry(&reports[0], "render").coverage, Coverage::Tested);
        assert_eq!(entry(&reports[0], "render").tests, ["TEST-RENDER"]);
        assert!(reports[1].findings.is_empty());
        assert_eq!(reports[1].summary[1], ("test_count", json!(1)));
    }

    #[test]
    fn another_package_and_an_unknown_reader_package_do_not_claim_coverage() {
        for test_source in [
            "(in-package :other) (deftest test-render () 1)",
            "(deftest test-render () 1)",
        ] {
            let reports = reports(&[
                ("(in-package :app) (defun render () 1)", Dialect::CommonLisp),
                (test_source, Dialect::CommonLisp),
            ]);
            assert_eq!(entry(&reports[0], "render").coverage, Coverage::Untested);
            assert_eq!(
                entry(&reports[1], "test-render").coverage,
                Coverage::Unattributed
            );
        }
    }

    #[test]
    fn explicit_package_qualified_test_name_attributes_to_that_package() {
        let reports = reports(&[
            ("(in-package :app) (defun render () 1)", Dialect::CommonLisp),
            (
                "(in-package :other) (deftest app::test-render () 1)",
                Dialect::CommonLisp,
            ),
        ]);
        assert_eq!(entry(&reports[0], "render").coverage, Coverage::Tested);
        assert!(reports[1].findings.is_empty());
    }

    #[test]
    fn duplicate_subjects_leave_the_test_unattributed() {
        let reports = reports(&[
            ("(in-package :app) (defun render () 1)", Dialect::CommonLisp),
            ("(in-package :app) (defun render () 2)", Dialect::CommonLisp),
            (
                "(in-package :app) (deftest test-render () 1)",
                Dialect::CommonLisp,
            ),
        ]);
        assert_eq!(entry(&reports[0], "render").coverage, Coverage::Untested);
        assert_eq!(entry(&reports[1], "render").coverage, Coverage::Untested);
        assert_eq!(
            entry(&reports[2], "test-render").coverage,
            Coverage::Unattributed
        );
    }

    #[test]
    fn emacs_lisp_cross_file_names_are_case_sensitive() {
        let reports = reports(&[
            (
                "(defun render () 1) (defun Render () 2)",
                Dialect::EmacsLisp,
            ),
            ("(ert-deftest test-render () t)", Dialect::EmacsLisp),
        ]);
        assert_eq!(reports[0].findings[0].name, "render");
        assert_eq!(reports[0].findings[0].coverage, Coverage::Tested);
        assert_eq!(reports[0].findings[1].name, "Render");
        assert_eq!(reports[0].findings[1].coverage, Coverage::Untested);
        assert!(reports[1].findings.is_empty());
    }

    #[test]
    fn a_test_in_another_dialect_does_not_cover_a_definition() {
        let reports = reports(&[
            ("(defun render () 1)", Dialect::CommonLisp),
            ("(ert-deftest test-render () t)", Dialect::EmacsLisp),
        ]);
        assert_eq!(entry(&reports[0], "render").coverage, Coverage::Untested);
        assert_eq!(
            entry(&reports[1], "test-render").coverage,
            Coverage::Unattributed
        );
    }
}
