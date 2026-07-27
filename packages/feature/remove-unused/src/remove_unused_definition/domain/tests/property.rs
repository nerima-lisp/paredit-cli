use super::*;

proptest! {
    #[test]
    fn pbt_unused_private_function_rewrite_is_parseable(name in lisp_symbol_strategy()) {
        let form = format!("(defun {name} () 1)");
        let text = format!("(in-package #:app)\n{form}\n(defun live () 2)\n");
        let definitions = vec![
            definition(&text, &form, &name, DefinitionCategory::Function),
            definition(&text, "(defun live () 2)", "live", DefinitionCategory::Function),
        ];

        let plan = plan_remove_unused_definitions(request_for(&text, definitions)).expect("plan should build");

        prop_assert!(plan.changed);
        prop_assert_eq!(plan.removal_count, 2);
        prop_assert!(!plan.files[0].rewritten.contains(&form));
        prop_assert!(SyntaxTree::parse(&plan.files[0].rewritten).is_ok());
    }
}
