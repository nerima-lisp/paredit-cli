use super::*;

#[test]
fn unused_candidates_ignore_self_references_and_count_external_references() {
    let input = "(defun used () :ok)\n\
             (defun caller () (used))\n\
             (defun recursive-unused () (recursive-unused))\n";
    let tree = SyntaxTree::parse(input).expect("parse input");
    let parsed = build_parsed_definition_file(
        PathBuf::from("core.lisp"),
        Dialect::CommonLisp,
        &tree,
        input,
    )
    .expect("build parsed file");

    let reports = collect_unused_definition_candidates(&[parsed])
        .expect("collect unused definition candidates");
    let used = reports[0]
        .definitions
        .iter()
        .find(|item| item.definition.name.as_deref() == Some("used"))
        .expect("used definition");
    let recursive_unused = reports[0]
        .definitions
        .iter()
        .find(|item| item.definition.name.as_deref() == Some("recursive-unused"))
        .expect("recursive definition");

    assert_eq!(used.references.len(), 1);
    assert_eq!(recursive_unused.references.len(), 0);
    assert_eq!(unused_definition_candidate_count(&reports), 2);
}

#[test]
fn unused_candidates_count_unqualified_references_to_package_qualified_common_lisp_definition() {
    let input = "(defun cl-user:used () :ok)\n\
             (defun caller () (used))\n";
    let tree = SyntaxTree::parse(input).expect("parse input");
    let parsed = build_parsed_definition_file(
        PathBuf::from("core.lisp"),
        Dialect::CommonLisp,
        &tree,
        input,
    )
    .expect("build parsed file");

    let reports = collect_unused_definition_candidates(&[parsed])
        .expect("collect unused definition candidates");
    let used = reports[0]
        .definitions
        .iter()
        .find(|item| item.definition.name.as_deref() == Some("cl-user:used"))
        .expect("qualified used definition");

    assert_eq!(used.references.len(), 1);
    assert!(!used.references.is_empty());
}

#[test]
fn unused_candidates_count_function_quote_and_quoted_dispatch_table_references() {
    // A definition reachable only through `#'name` (function-namespace
    // capture) or a bare `'name` inside a quoted dispatch table/alist is
    // still live. `remove-unused-definitions` already treats both as
    // references (see remove_unused_definition::candidates); this pins the
    // same behavior for `unused-definition-report`, which used to run a
    // narrower, atoms-only scan and disagree with it.
    let input = "(defun handler () 1)\n\
                 (defun dispatch-target () 2)\n\
                 (defparameter *routes* (list (cons :get #'handler)))\n\
                 (defparameter *table* '((:dispatch 'dispatch-target)))\n";
    let tree = SyntaxTree::parse(input).expect("parse input");
    let parsed = build_parsed_definition_file(
        PathBuf::from("core.lisp"),
        Dialect::CommonLisp,
        &tree,
        input,
    )
    .expect("build parsed file");

    let reports = collect_unused_definition_candidates(&[parsed])
        .expect("collect unused definition candidates");
    let handler = reports[0]
        .definitions
        .iter()
        .find(|item| item.definition.name.as_deref() == Some("handler"))
        .expect("handler definition");
    let dispatch_target = reports[0]
        .definitions
        .iter()
        .find(|item| item.definition.name.as_deref() == Some("dispatch-target"))
        .expect("dispatch-target definition");

    assert!(!handler.references.is_empty());
    assert!(!dispatch_target.references.is_empty());
}

#[test]
fn policy_reports_fail_on_unused_and_required_minimum() {
    let input = "(defun stale () :stale)\n";
    let tree = SyntaxTree::parse(input).expect("parse input");
    let parsed = build_parsed_definition_file(
        PathBuf::from("core.lisp"),
        Dialect::CommonLisp,
        &tree,
        input,
    )
    .expect("build parsed file");
    let reports = collect_unused_definition_candidates(&[parsed])
        .expect("collect unused definition candidates");

    let policy = evaluate_unused_definition_policy(
        UnusedDefinitionPolicyOptions::new(true, Some(2)).unwrap(),
        &reports,
    );

    assert_eq!(policy.definition_count, 1);
    assert_eq!(policy.candidate_count, 1);
    assert!(!policy.passed);
    assert_eq!(policy.violations.len(), 2);
}

#[test]
fn policy_ignores_protected_category_candidates_for_fail_on_unused() {
    // `deftest` (category `Test`) is invoked by name from a test runner, not
    // referenced by symbol from other Lisp forms, so it having zero direct
    // references is normal and should not itself trip `--fail-on-unused`.
    // The real dead code here — `stale-helper`, category `Function` — still
    // should.
    let input = "(deftest stale-test () (is t))\n\
                 (defun stale-helper () 1)\n";
    let tree = SyntaxTree::parse(input).expect("parse input");
    let parsed = build_parsed_definition_file(
        PathBuf::from("core.lisp"),
        Dialect::CommonLisp,
        &tree,
        input,
    )
    .expect("build parsed file");
    let reports = collect_unused_definition_candidates(&[parsed])
        .expect("collect unused definition candidates");

    assert_eq!(unused_definition_candidate_count(&reports), 2);
    assert_eq!(unused_definition_actionable_candidate_count(&reports), 1);

    let policy = evaluate_unused_definition_policy(
        UnusedDefinitionPolicyOptions::new(true, None).unwrap(),
        &reports,
    );

    assert_eq!(policy.candidate_count, 2);
    assert_eq!(policy.actionable_candidate_count, 1);
    assert!(!policy.passed);
    assert_eq!(policy.violations.len(), 1);
    assert!(policy.violations[0].contains("actionable_candidate_count 1"));
}

#[test]
fn policy_passes_fail_on_unused_when_only_protected_category_is_unreferenced() {
    let input = "(deftest stale-test () (is t))\n";
    let tree = SyntaxTree::parse(input).expect("parse input");
    let parsed = build_parsed_definition_file(
        PathBuf::from("core.lisp"),
        Dialect::CommonLisp,
        &tree,
        input,
    )
    .expect("build parsed file");
    let reports = collect_unused_definition_candidates(&[parsed])
        .expect("collect unused definition candidates");

    assert_eq!(unused_definition_candidate_count(&reports), 1);
    assert_eq!(unused_definition_actionable_candidate_count(&reports), 0);

    let policy = evaluate_unused_definition_policy(
        UnusedDefinitionPolicyOptions::new(true, None).unwrap(),
        &reports,
    );

    assert_eq!(policy.candidate_count, 1);
    assert_eq!(policy.actionable_candidate_count, 0);
    assert!(policy.passed);
    assert!(policy.violations.is_empty());
}
