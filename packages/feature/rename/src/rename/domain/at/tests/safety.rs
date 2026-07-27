use super::*;

#[test]
fn rejects_a_lexical_binding_target_that_already_exists_in_its_scope() {
    let input = "(let ((value 1) (count 2)) (+ value count))";

    let error = plan_rename_at(request(input, "value 1", "count")).expect_err("name conflict");

    assert_eq!(
        error.downcast_ref::<RenameAtError>(),
        Some(&RenameAtError::NameConflict)
    );
}

#[test]
fn rejects_global_rename_when_a_package_qualified_reference_would_be_rewritten() {
    let input = "(defun foo () 1) (other:foo)";

    let error = plan_rename_at(request(input, "foo ()", "bar")).expect_err("package reference");

    assert_eq!(
        error.downcast_ref::<RenameAtError>(),
        Some(&RenameAtError::PackageQualifiedReference)
    );
}
