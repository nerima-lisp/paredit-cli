use super::*;

proptest! {
    #[test]
    fn pbt_referenced_generated_function_is_not_unused(
        function_name in "[a-z][a-z0-9-]{0,12}",
        caller_name in "[a-z][a-z0-9-]{0,12}",
        arg_count in 0usize..8,
    ) {
        prop_assume!(function_name != caller_name);
        // Standard operator names (`do`, `if`, ...) cannot be defun'd in
        // conforming Common Lisp, and the scope-aware reference collector
        // rightly reads `(do ...)` as the special form rather than a call.
        prop_assume!(
            crate::domain::common_lisp::CommonLispOperator::from_head(&function_name).is_none()
        );
        prop_assume!(
            crate::domain::common_lisp::CommonLispOperator::from_head(&caller_name).is_none()
        );
        let params = (0..arg_count)
            .map(|index| format!("arg{index}"))
            .collect::<Vec<_>>();
        let args = (0..arg_count)
            .map(|_| ":value".to_owned())
            .collect::<Vec<_>>();
        let input = format!(
            "(defun {function_name} ({}) :ok)\n(defun {caller_name} () ({function_name} {}))\n",
            params.join(" "),
            args.join(" ")
        );
        let tree = SyntaxTree::parse(&input).expect("parse generated input");
        let parsed = build_parsed_definition_file(
            PathBuf::from("generated.lisp"),
            Dialect::CommonLisp,
            &tree,
            &input,
        )
        .expect("build generated parsed file");

        let reports = collect_unused_definition_candidates(&[parsed])
            .expect("collect unused definition candidates");
        let generated = reports[0]
            .definitions
            .iter()
            .find(|item| item.definition.name.as_deref() == Some(function_name.as_str()))
            .expect("generated function definition");

        prop_assert_eq!(generated.definition.parameter_count, Some(arg_count));
        prop_assert_eq!(generated.references.len(), 1);
    }
}
