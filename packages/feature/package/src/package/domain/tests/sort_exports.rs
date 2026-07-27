use super::*;

#[test]
fn sorts_package_exports_without_moving_other_options() {
    let input =
        "(defpackage #:demo\n  (:use #:cl)\n  (:export #:z #:a #:m)\n  (:import-from #:x #:y))\n";
    let plan = plan_sort_package_exports(SortPackageExportsRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: Some(&SymbolName::new("demo").unwrap()),
    })
    .unwrap();

    assert!(plan.changed);
    assert_eq!(plan.exports.len(), 1);
    assert_eq!(plan.exports[0].old_symbols, ["#:z", "#:a", "#:m"]);
    assert_eq!(plan.exports[0].new_symbols, ["#:a", "#:m", "#:z"]);
    assert!(plan.rewritten.contains("(:export #:a #:m #:z)"));
    assert!(plan.rewritten.contains("(:import-from #:x #:y)"));
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn sorts_exports_only_in_target_defpackage_among_mixed_top_level_forms() {
    let input = "(in-package #:cl-user)\n(defpackage #:other (:export #:z #:a))\n42\n(defpackage #:target (:export #:y #:b))\n(main)\n";
    let plan = plan_sort_package_exports(SortPackageExportsRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: Some(&SymbolName::new("target").unwrap()),
    })
    .unwrap();

    assert_eq!(plan.exports.len(), 1);
    assert_eq!(plan.exports[0].package, "#:target");
    assert_eq!(
        plan.rewritten,
        "(in-package #:cl-user)\n(defpackage #:other (:export #:z #:a))\n42\n(defpackage #:target (:export #:b #:y))\n(main)\n"
    );
}

#[test]
fn sorted_package_exports_are_idempotent() {
    let input = "(defpackage #:demo (:export #:a #:b #:c))\n";
    let plan = plan_sort_package_exports(SortPackageExportsRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: None,
    })
    .unwrap();

    assert!(!plan.changed);
    assert_eq!(plan.rewritten, input);
    assert_eq!(plan.exports[0].new_symbols, ["#:a", "#:b", "#:c"]);
}

#[test]
fn sort_package_exports_keeps_leading_comments_with_their_symbols() {
    let input = "\
(defpackage #:demo
  (:export
   ;; group b
   #:zeta
   ;; group a
   #:alpha
   #:beta))
";
    let plan = plan_sort_package_exports(SortPackageExportsRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: None,
    })
    .unwrap();

    assert!(plan.changed);
    assert_eq!(plan.exports[0].new_symbols, ["#:alpha", "#:beta", "#:zeta"]);
    let expected = "\
(defpackage #:demo
  (:export
   ;; group a
   #:alpha
   #:beta
   ;; group b
   #:zeta))
";
    assert_eq!(plan.rewritten, expected);
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn sort_package_exports_keeps_trailing_same_line_comment_and_stays_balanced() {
    let input = "(defpackage #:demo\n  (:export\n   #:zeta ; last one\n   #:alpha))\n";
    let plan = plan_sort_package_exports(SortPackageExportsRequest {
        input,
        dialect: Dialect::CommonLisp,
        package: None,
    })
    .unwrap();

    assert!(plan.changed);
    assert_eq!(plan.exports[0].new_symbols, ["#:alpha", "#:zeta"]);
    // The trailing comment stays glued to #:zeta; the closing delimiters that
    // followed the region are pushed to a fresh line so they are not commented out.
    let expected = "(defpackage #:demo\n  (:export\n   #:alpha\n   #:zeta ; last one\n   ))\n";
    assert_eq!(plan.rewritten, expected);
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_sort_package_exports_output_remains_parseable_and_ordered(
        package in package_name_strategy(),
        mut symbols in prop::collection::vec(symbol_strategy(), 2..8),
    ) {
        symbols.sort();
        symbols.dedup();
        prop_assume!(symbols.len() >= 2);
        let reversed = symbols.iter().rev().map(|symbol| format!("#:{symbol}")).collect::<Vec<_>>();
        let input = format!("(defpackage #:{package} (:export {}))\n", reversed.join(" "));
        let plan = plan_sort_package_exports(SortPackageExportsRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            package: None,
        }).unwrap();
        let expected = symbols.iter().map(|symbol| format!("#:{symbol}")).collect::<Vec<_>>();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert_eq!(plan.exports.len(), 1);
        prop_assert_eq!(&plan.exports[0].new_symbols, &expected);
    }
}
