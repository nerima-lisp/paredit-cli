use super::*;

#[test]
fn rename_package_updates_designators_and_qualified_prefixes() {
    let input = "(defpackage #:old.pkg\n\
                       (:use #:cl #:old.pkg)\n\
                       (:import-from #:old.pkg #:thing)\n\
                       (:shadowing-import-from #:old.pkg #:shadowed)\n\
                       (:local-nicknames (#:local #:old.pkg)))\n\
                     (in-package #:old.pkg)\n\
                     (defun call () (old.pkg:thing old.pkg::internal :old.pkg))\n\
                     \"old.pkg:string\"\n";
    let plan = plan_rename_package(RenamePackageRequest {
        input,
        dialect: Dialect::CommonLisp,
        from: &SymbolName::new("old.pkg").unwrap(),
        to: &SymbolName::new("new.pkg").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.occurrences.len(), 8);
    assert!(plan.changed);
    assert!(plan.rewritten.contains("(defpackage #:new.pkg"));
    assert!(
        plan.rewritten
            .contains("(:shadowing-import-from #:new.pkg #:shadowed)")
    );
    assert!(plan.rewritten.contains("(in-package #:new.pkg)"));
    assert!(plan.rewritten.contains("new.pkg:thing"));
    assert!(plan.rewritten.contains("new.pkg::internal"));
    assert!(plan.rewritten.contains(":old.pkg"));
    assert!(plan.rewritten.contains("\"old.pkg:string\""));
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_rename_package_output_remains_parseable_and_skips_literals(
        from in package_name_strategy(),
        to in package_name_strategy(),
        symbol in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        let input = format!(
            "(defpackage #:{from} (:use #:cl #:{from}) (:import-from #:{from} #:{symbol}))\n\
             (in-package #:{from})\n\
             (defun call () ({from}:{symbol} :{from} \"{from}:literal\"))\n"
        );
        let from_package = SymbolName::new(from.clone()).unwrap();
        let to_package = SymbolName::new(to.clone()).unwrap();
        let plan = plan_rename_package(RenamePackageRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            from: &from_package,
            to: &to_package,
        }).unwrap();
        let expected_defpackage = format!("(defpackage #:{to}");
        let expected_in_package = format!("(in-package #:{to})");
        let expected_qualified = format!("{to}:{symbol}");
        let expected_keyword = format!(":{from}");
        let expected_literal = format!("\"{from}:literal\"");

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert!(plan.rewritten.contains(&expected_defpackage));
        prop_assert!(plan.rewritten.contains(&expected_in_package));
        prop_assert!(plan.rewritten.contains(&expected_qualified));
        prop_assert!(plan.rewritten.contains(&expected_keyword));
        prop_assert!(plan.rewritten.contains(&expected_literal));
    }
}
