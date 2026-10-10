use super::*;
use crate::test_risk_report::usecase::evaluate_test_risk_policy;

fn report(inputs: &[(&str, Dialect, &str)]) -> TestRiskReport {
    let trees = inputs
        .iter()
        .map(|(_, dialect, source)| SyntaxTree::parse_with_dialect(source, *dialect).unwrap())
        .collect::<Vec<_>>();
    let files = inputs
        .iter()
        .zip(&trees)
        .map(|((path, dialect, _), tree)| (Path::new(path), *dialect, tree))
        .collect::<Vec<_>>();
    build_test_risk_report(&files).unwrap()
}

#[test]
fn test_risk_excludes_linked_control_and_ranks_only_functions() {
    let report = report(&[(
        "app.lisp",
        Dialect::CommonLisp,
        "(in-package :app) (defun linked () (if t (list (list 1)) 0)) \
         (deftest test-linked () 1) (defun unlinked () 1) \
         (defmacro expansion () 1) (defgeneric generic ()) (defmethod generic () 1)",
    )]);
    assert_eq!(report.files[0].function_count, Some(2));
    assert_eq!(report.files[0].linked_function_count, Some(1));
    assert_eq!(report.files[0].unlinked_function_count, Some(1));
    assert_eq!(report.ranked.len(), 1);
    assert_eq!(report.ranked[0].name, "UNLINKED");
    assert_eq!(
        report.ranked[0].complexity.category,
        DefinitionCategory::Function
    );
}

#[test]
fn test_risk_links_across_files_in_the_same_package_only() {
    let report = report(&[
        (
            "a.lisp",
            Dialect::CommonLisp,
            "(in-package :app) (defun work () 1)",
        ),
        (
            "b.lisp",
            Dialect::CommonLisp,
            "(in-package :other) (defun work () 1)",
        ),
        (
            "tests.lisp",
            Dialect::CommonLisp,
            "(in-package :app) (deftest test-work () 1)",
        ),
    ]);
    assert_eq!(report.ranked.len(), 1);
    assert_eq!(report.ranked[0].file, Path::new("b.lisp"));
    assert_eq!(report.files[0].linked_function_count, Some(1));
    assert_eq!(report.files[1].linked_function_count, Some(0));
}

#[test]
fn test_risk_unknown_package_is_file_local() {
    let report = report(&[
        ("a.lisp", Dialect::CommonLisp, "(defun work () 1)"),
        ("b.lisp", Dialect::CommonLisp, "(deftest test-work () 1)"),
        (
            "c.lisp",
            Dialect::CommonLisp,
            "(defun own () 1) (deftest test-own () 1)",
        ),
    ]);
    assert_eq!(report.ranked.len(), 1);
    assert_eq!(report.ranked[0].name, "WORK");
    assert_eq!(report.files[2].linked_function_count, Some(1));
}

#[test]
fn test_risk_ambiguous_subjects_are_not_assumed_linked() {
    let report = report(&[
        (
            "a.lisp",
            Dialect::CommonLisp,
            "(in-package :app) (defun work () 1)",
        ),
        (
            "b.lisp",
            Dialect::CommonLisp,
            "(in-package :app) (defun work () 2)",
        ),
        (
            "tests.lisp",
            Dialect::CommonLisp,
            "(in-package :app) (deftest test-work () 1)",
        ),
    ]);
    assert_eq!(report.ranked.len(), 2);
    assert!(
        report
            .files
            .iter()
            .all(|file| file.linked_function_count == Some(0))
    );
}

#[test]
fn test_risk_mixed_dialects_do_not_share_name_identity() {
    let report = report(&[
        ("a.lisp", Dialect::CommonLisp, "(defun work () 1)"),
        (
            "b.el",
            Dialect::EmacsLisp,
            "(defun work () 1) (ert-deftest test-work () 1)",
        ),
    ]);
    assert_eq!(report.ranked.len(), 1);
    assert_eq!(report.ranked[0].dialect, Dialect::CommonLisp);
    assert_eq!(report.files[1].linked_function_count, Some(1));
}

#[test]
fn test_risk_unmodelled_dialect_has_unknown_counts() {
    let report = report(&[("a.scm", Dialect::Scheme, "(define (work) 1)")]);
    assert!(report.ranked.is_empty());
    let file = &report.files[0];
    assert!(!file.modelled);
    assert_eq!(file.function_count, None);
    assert_eq!(file.linked_function_count, None);
    assert_eq!(file.unlinked_function_count, None);
}

#[test]
fn test_risk_ranks_by_score_and_ties_by_file_then_source_position() {
    let inputs = [
        (
            "z.lisp",
            Dialect::CommonLisp,
            "(defun deep () (list (list (list 1))))",
        ),
        (
            "b.lisp",
            Dialect::CommonLisp,
            "(defun z () 1) (defun a () 1)",
        ),
        ("a.lisp", Dialect::CommonLisp, "(defun equal () 1)"),
    ];
    let forward = report(&inputs);
    let reverse = report(&inputs.into_iter().rev().collect::<Vec<_>>());
    let projection = |report: &TestRiskReport| {
        report
            .ranked
            .iter()
            .map(|entry| {
                (
                    entry.file.clone(),
                    entry.name.clone(),
                    entry.complexity.complexity_score,
                    entry.complexity.span,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(projection(&forward), projection(&reverse));
    assert_eq!(
        forward
            .ranked
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["DEEP", "EQUAL", "Z", "A"]
    );
    assert!(
        forward.ranked[0].complexity.complexity_score
            > forward.ranked[1].complexity.complexity_score
    );
}

#[test]
fn test_risk_source_coordinates_identify_the_actual_definition() {
    let source = "; λ\n\n(defun work () (list 1))\n";
    let report = report(&[("a.lisp", Dialect::CommonLisp, source)]);
    let entry = &report.ranked[0];
    assert_eq!(entry.line, 3);
    assert_eq!(
        &source[entry.complexity.span.start().get()..entry.complexity.span.end().get()],
        "(defun work () (list 1))"
    );
    assert!(entry.complexity.max_depth > 0);
    assert!(entry.complexity.list_count > 0);
}

#[test]
fn test_risk_policy_uses_all_findings_and_linked_control_passes() {
    let missing = report(&[(
        "a.lisp",
        Dialect::CommonLisp,
        "(defun a () 1) (defun b () 1)",
    )]);
    let policy = evaluate_test_risk_policy(true, &missing);
    assert!(!policy.passed);
    assert_eq!(policy.finding_count, 2);
    assert_eq!(policy.gate, Some("--fail-on-untested"));
    assert!(evaluate_test_risk_policy(false, &missing).passed);
    let linked = report(&[(
        "a.lisp",
        Dialect::CommonLisp,
        "(defun a () 1) (deftest test-a () 1)",
    )]);
    assert!(evaluate_test_risk_policy(true, &linked).passed);
}

#[test]
fn test_risk_empty_fileset_is_empty() {
    let report = report(&[]);
    assert!(report.files.is_empty());
    assert!(report.ranked.is_empty());
    assert!(evaluate_test_risk_policy(true, &report).passed);
}
