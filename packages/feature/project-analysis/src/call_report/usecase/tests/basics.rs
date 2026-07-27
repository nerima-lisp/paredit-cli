use super::*;

#[test]
fn reports_calls_without_definition_forms_by_default() {
    let tree = parse("(defun f (x) (g x) (h))\n(g 1)");
    let calls = build_call_report(&tree, Dialect::CommonLisp, None, false).unwrap();

    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].head, "g");
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("f"));
    assert_eq!(calls[1].head, "h");
    assert_eq!(calls[1].argument_count, 0);
    assert_eq!(calls[1].enclosing_definition.as_deref(), Some("f"));
    assert_eq!(calls[2].head, "g");
    assert_eq!(calls[2].enclosing_definition, None);
}

#[test]
fn can_include_definition_forms_for_inventory_reports() {
    let tree = parse("(defun f (x) (g x))");
    let calls = build_call_report(&tree, Dialect::CommonLisp, None, true).unwrap();

    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].head, "defun");
    assert_eq!(calls[0].category, Some(DefinitionCategory::Function));
    assert_eq!(calls[1].head, "g");
    assert_eq!(calls[1].category, None);
}

#[test]
fn filters_by_symbol() {
    let tree = parse("(defun f (x) (g x) (h x) (g 1 2))");
    let symbol = SymbolName::new("g").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| call.head == "g"));
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[1].argument_count, 2);
}
