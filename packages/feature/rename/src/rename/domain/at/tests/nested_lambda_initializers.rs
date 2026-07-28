use super::*;

#[test]
fn renames_outer_binding_in_nested_callable_optional_initializers() {
    for (form, expected) in [
        (
            "flet",
            "(let ((renamed 1)) (flet ((work (&optional (x renamed)) x)) renamed))",
        ),
        (
            "labels",
            "(let ((renamed 1)) (labels ((work (&optional (x renamed)) x)) renamed))",
        ),
        (
            "macrolet",
            "(let ((renamed 1)) (macrolet ((work (&optional (x renamed)) x)) renamed))",
        ),
    ] {
        let input = format!("(let ((x 1)) ({form} ((work (&optional (x x)) x)) x))");
        let plan = plan_rename_at(request(&input, "x 1", "renamed")).expect("plan");

        assert_eq!(plan.rewritten, expected, "{form}");
    }
}

#[test]
fn renames_outer_binding_in_defmethod_optional_initializer() {
    let input = "(let ((x 1)) (defmethod work (&optional (x x)) x) x)";
    let plan = plan_rename_at(request(input, "x 1", "renamed")).expect("plan");

    assert_eq!(
        plan.rewritten,
        "(let ((renamed 1)) (defmethod work (&optional (x renamed)) x) renamed)"
    );
}
