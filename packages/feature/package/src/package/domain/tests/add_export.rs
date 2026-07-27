use super::*;

#[test]
fn adds_export_to_existing_option() {
    let input = "(defpackage #:demo\n  (:use #:cl)\n  (:export #:old))\n";
    let plan = plan_add_export(AddExportRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: Some(&SymbolName::new("demo").unwrap()),
        symbol: &SymbolName::new("#:new").unwrap(),
    })
    .unwrap();

    assert!(plan.changed);
    assert!(!plan.already_exported);
    assert_eq!(plan.package, "#:demo");
    assert!(plan.rewritten.contains("(:export #:old #:new)"));
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn adds_export_with_hash_colon_prefix_even_when_symbol_argument_is_bare() {
    let input = "(defpackage #:demo\n  (:use #:cl)\n  (:export #:old))\n";
    let plan = plan_add_export(AddExportRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: Some(&SymbolName::new("demo").unwrap()),
        symbol: &SymbolName::new("new").unwrap(),
    })
    .unwrap();

    assert!(plan.changed);
    assert!(plan.rewritten.contains("(:export #:old #:new)"));
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn adds_export_option_with_hash_colon_prefix_when_symbol_argument_is_bare() {
    let input = "(defpackage #:demo\n  (:use #:cl))\n";
    let plan = plan_add_export(AddExportRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: Some(&SymbolName::new("demo").unwrap()),
        symbol: &SymbolName::new("new").unwrap(),
    })
    .unwrap();

    assert!(plan.changed);
    assert!(plan.rewritten.contains("(:export #:new)"));
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn add_export_is_idempotent_for_existing_symbol() {
    let input = "(defpackage #:demo (:export #:main))\n";
    let plan = plan_add_export(AddExportRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: Some(&SymbolName::new(":demo").unwrap()),
        symbol: &SymbolName::new("main").unwrap(),
    })
    .unwrap();

    assert!(!plan.changed);
    assert!(plan.already_exported);
    assert_eq!(plan.rewritten, input);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_add_export_output_remains_parseable(package in package_name_strategy(), symbol in symbol_strategy()) {
        let input = format!("(defpackage #:{package} (:use #:cl))\n");
        let export = format!("#:{symbol}");
        let package_name = SymbolName::new(package.clone()).unwrap();
        let export_symbol = SymbolName::new(export.clone()).unwrap();
        let plan = plan_add_export(AddExportRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            package: Some(&package_name),
            symbol: &export_symbol,
        }).unwrap();
        let expected_export = format!("(:export {export})");

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert!(plan.rewritten.contains(&expected_export));
    }
}
