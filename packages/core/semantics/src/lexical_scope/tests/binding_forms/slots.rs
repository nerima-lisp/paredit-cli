use super::*;

#[test]
fn with_slots_bindings_shadow_body_but_not_instance_form() {
    let input = "(list slot (with-slots (slot (alias slot)) slot (list slot alias)) slot)";

    assert_eq!(reference_texts(input, "slot"), vec!["slot", "slot", "slot"]);
}

#[test]
fn with_accessors_bindings_shadow_body_but_not_instance_form() {
    let input = "(list value (with-accessors ((value get-value) (alias value)) value (list value alias)) value)";

    assert_eq!(
        reference_texts(input, "value"),
        vec!["value", "value", "value"]
    );
}
