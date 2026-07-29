//! Turning `inspect test-map`'s untested entries into `deftest` skeletons.
//!
//! `inspect test-map` already does the hard part: pairing definitions with
//! the tests that name them, by the same conventions this crate's own test
//! suite uses (`test-render`, `render-test`, `render-tests`). This slice
//! reuses that pairing rather than re-deriving it, and generates a stub named
//! `test-<subject>` — the first convention `test-map` checks — so a
//! generated stub is recognized as coverage the moment it is written.
//!
//! What the stub cannot know is how to call the subject: `test-map` pairs by
//! name only, not by arity or by what a passing call looks like. The body is
//! a placeholder that fails loudly (`(is nil)`) with a comment naming the
//! call to write, rather than a stub that silently passes and looks like
//! coverage without being any.

use std::collections::BTreeSet;

use paredit_feature_project_inventory::test_map_report::usecase::{Coverage, CoverageEntry};

/// One generated `deftest` skeleton for one untested definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestStub {
    pub subject: String,
    pub generated: String,
}

#[must_use]
pub fn build_test_stubs(entries: &[CoverageEntry]) -> Vec<TestStub> {
    // A `defgeneric` and its `defmethod` are two separate testable
    // definitions to `test-map` (a method can be untested while its generic
    // is not), but they share a name, and a name is all a `deftest` has to
    // go on. Two stubs called `test-speak` would not both compile into one
    // file — the second silently redefines the first — so only the first
    // untested entry per name becomes a stub.
    let mut seen = BTreeSet::new();
    entries
        .iter()
        .filter(|entry| entry.coverage == Coverage::Untested)
        .filter_map(|entry| {
            let subject = entry.name.to_ascii_lowercase();
            seen.insert(subject.clone()).then(|| TestStub {
                generated: format!(
                    "(deftest test-{subject} ()\n  \"TODO: verify {subject}.\"\n  ;; TODO: call ({subject} ...) with representative arguments and assert the result.\n  (is nil))\n\n"
                ),
                subject,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_feature_project_inventory::test_map_report::usecase::build_test_map_report;
    use std::path::Path;

    fn stubs(source: &str) -> Vec<TestStub> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let report = build_test_map_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree);
        build_test_stubs(&report.findings)
    }

    #[test]
    fn an_untested_definition_gets_a_stub_named_by_convention() {
        let found = stubs("(defun render (x) x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].subject, "render");
        assert!(found[0].generated.starts_with("(deftest test-render ()"));
    }

    #[test]
    fn a_tested_definition_gets_no_stub() {
        let found = stubs("(defun render (x) x)\n(deftest test-render () (is t))");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_stub_body_fails_rather_than_silently_passing() {
        let found = stubs("(defun render (x) x)");
        assert!(found[0].generated.contains("(is nil)"));
    }

    #[test]
    fn a_package_definition_needs_no_stub() {
        let found = stubs("(defpackage :app (:use :cl))");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_generic_and_its_untested_method_share_one_stub_by_name() {
        let found = stubs("(defgeneric speak (x))\n(defmethod speak ((x fish)) 1)");
        assert_eq!(
            found.iter().filter(|stub| stub.subject == "speak").count(),
            1,
            "{found:?}"
        );
    }
}
