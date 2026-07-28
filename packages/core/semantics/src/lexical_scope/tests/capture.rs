use super::*;

fn parse_path(path: &str) -> Path {
    path.parse().expect("path")
}

fn captured_names(
    input: &str,
    parsed_dialect: Dialect,
    capture_dialect: Dialect,
    value_path: &str,
    reference_path: &str,
) -> Vec<String> {
    let tree = SyntaxTree::parse_with_dialect(input, parsed_dialect).expect("dialect parse");
    let scope = tree.select_path(&parse_path("0")).expect("scope");
    let value = tree.select_path(&parse_path(value_path)).expect("value");
    let reference = tree
        .select_path(&parse_path(reference_path))
        .expect("reference");

    value_capture(
        capture_dialect,
        input,
        scope.span(),
        &SymbolName::new("target").expect("binding"),
        &value.view(),
        &[reference.span()],
    )
    .into_iter()
    .map(|symbol| symbol.as_str().to_owned())
    .collect()
}

#[test]
fn common_lisp_dispatch_form_uses_the_dialect_reader_shape() {
    let input = r"(let ((target external)) #S(holder :value external) target)";
    let dialect_tree =
        SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("Common Lisp parse");
    let generic_tree = SyntaxTree::parse(input).expect("legacy parse");
    assert_eq!(dialect_tree.root_view().children[0].children.len(), 4);
    assert_eq!(generic_tree.root_view().children[0].children.len(), 5);

    assert!(
        captured_names(
            input,
            Dialect::CommonLisp,
            Dialect::CommonLisp,
            "0.1.0.1",
            "0.3",
        )
        .is_empty()
    );
}

#[test]
fn clojure_discard_and_tagged_literal_use_the_dialect_reader_shape() {
    let input = r#"(let [target external] #_ignored #inst "2020-01-01" target)"#;
    let dialect_tree =
        SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("Clojure parse");
    let generic_tree = SyntaxTree::parse(input).expect("legacy parse");
    assert_eq!(dialect_tree.root_view().children[0].children.len(), 4);
    assert_eq!(generic_tree.root_view().children[0].children.len(), 5);

    assert!(captured_names(input, Dialect::Clojure, Dialect::Clojure, "0.1.1", "0.3",).is_empty());
}

#[test]
fn dialect_valid_scope_that_the_generic_reader_rejects_is_not_false_safe() {
    let input = "(let [target external]\n  # unmatched )\n  (let [external 1] target))";
    assert!(SyntaxTree::parse(input).is_err());
    SyntaxTree::parse_with_dialect(input, Dialect::Janet).expect("Janet parse");

    assert_eq!(
        captured_names(input, Dialect::Janet, Dialect::Janet, "0.1.1", "0.2.2",),
        ["external"]
    );
}

#[test]
fn unknown_dialect_returns_all_free_variables_as_unsafe() {
    let input = "(let ((target external)) target)";
    assert_eq!(
        captured_names(
            input,
            Dialect::CommonLisp,
            Dialect::Unknown,
            "0.1.0.1",
            "0.2",
        ),
        ["external"]
    );
}

#[test]
fn scheme_variadic_lambda_parameter_captures_spliced_value() {
    let input = "(let ((target external)) (lambda external target))";

    assert_eq!(
        captured_names(input, Dialect::Scheme, Dialect::Scheme, "0.1.0.1", "0.2.2",),
        ["external"]
    );
}

#[test]
fn clojure_named_multi_arity_fn_captures_spliced_value() {
    let input = "(let [target recur-name] (fn recur-name ([value] target)))";

    assert_eq!(
        captured_names(
            input,
            Dialect::Clojure,
            Dialect::Clojure,
            "0.1.1",
            "0.2.2.1",
        ),
        ["recur-name"]
    );
}
