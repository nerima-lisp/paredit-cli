use proptest::prelude::*;

use super::*;

fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("reserved symbol", |name| {
        !matches!(
            name.as_str(),
            "let"
                | "let*"
                | "lambda"
                | "fn"
                | "defun"
                | "defmacro"
                | "nil"
                | "t"
                | "true"
                | "false"
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn pbt_infers_each_free_symbol_once_and_excludes_bound_symbols(
        outer_a in symbol_strategy(),
        outer_b in symbol_strategy(),
        local in symbol_strategy(),
    ) {
        prop_assume!(outer_a != outer_b);
        prop_assume!(outer_a != local);
        prop_assume!(outer_b != local);

        let input = format!("(+ {outer_a} {outer_b} (let (({local} {outer_a})) (+ {local} {outer_b})))");
        let tree = SyntaxTree::parse(&input).expect("parse generated input");
        let selection = tree
            .select_path(&Path::from_indexes(vec![0]))
            .expect("select generated input");

        let inferred = infer_extract_function_params(Dialect::CommonLisp, &selection.view(), &[]);

        prop_assert_eq!(inferred, vec![outer_a, outer_b]);
    }

    #[test]
    fn pbt_explicit_params_are_not_inferred_again(
        outer_a in symbol_strategy(),
        outer_b in symbol_strategy(),
    ) {
        prop_assume!(outer_a != outer_b);

        let input = format!("(+ {outer_a} {outer_b} {outer_a})");
        let tree = SyntaxTree::parse(&input).expect("parse generated input");
        let selection = tree
            .select_path(&Path::from_indexes(vec![0]))
            .expect("select generated input");
        let explicit = vec![outer_a];

        let inferred = infer_extract_function_params(Dialect::CommonLisp, &selection.view(), &explicit);

        prop_assert_eq!(inferred, vec![outer_b]);
    }

    #[test]
    fn pbt_planned_extract_function_output_remains_parseable(
        outer_a in symbol_strategy(),
        outer_b in symbol_strategy(),
    ) {
        prop_assume!(outer_a != outer_b);

        let input = format!("(defun source () (+ {outer_a} {outer_b}))");
        let tree = SyntaxTree::parse(&input).expect("parse generated input");
        let selection = tree
            .select_path(&Path::from_indexes(vec![0, 3]))
            .expect("select generated input");

        let plan = plan_extract_function(ExtractFunctionRequest {
            input: &input,
            selection,
            path: Some(Path::from_indexes(vec![0, 3])),
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("extracted").expect("symbol fixture"),
            explicit_params: Vec::new(),
            infer_params: true,
            insert: ExtractFunctionInsert::Append,
            anchor_path: None,
        })
        .expect("plan generated extraction");

        prop_assert!(SyntaxTree::parse(&plan.rewritten).is_ok());
        prop_assert_eq!(plan.params, vec![outer_a, outer_b]);
    }
}
