use super::*;

#[test]
fn rejects_unknown_dialect_before_extract_function_planning() {
    let input = "(+ width height)";
    let tree = SyntaxTree::parse(input).expect("parse fixture");
    let path = Path::from_indexes(vec![0]);
    let selection = tree.select_path(&path).expect("select fixture");

    assert!(infer_extract_function_params(Dialect::Unknown, &selection.view(), &[]).is_empty());

    let error = plan_extract_function(ExtractFunctionRequest {
        input,
        selection,
        path: Some(path),
        dialect: Dialect::Unknown,
        name: SymbolName::new("area").expect("symbol fixture"),
        explicit_params: Vec::new(),
        infer_params: true,
        insert: ExtractFunctionInsert::Append,
        anchor_path: None,
    })
    .expect_err("unknown dialect should be rejected");

    assert!(
        error
            .to_string()
            .contains("extract-function is not supported for this dialect")
    );
}

#[test]
fn plans_extract_function_with_inferred_params() {
    let plan = plan_at(
        "(defun render () (+ width height))",
        &[0, 3],
        "area",
        &[],
        true,
    );

    assert_eq!(plan.call, "(area width height)");
    assert_eq!(
        plan.definition,
        "(defun area (width height) (+ width height))"
    );
    assert_eq!(plan.inferred_params, vec!["width", "height"]);
    assert!(plan.changed);
    SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
        .expect("rewritten output remains parseable");
}

#[test]
fn extract_function_preserves_dialect_reader_collisions() {
    let cases = [(
        Dialect::Janet,
        "(+ width height)\n# ignored ))",
        "(area width height)",
        "(defn area [width height] (+ width height))",
    )];

    for (dialect, input, expected_call, expected_definition) in cases {
        let plan = plan_at_dialect(dialect, input, &[0], "area", &["width", "height"], false);

        assert_eq!(plan.call, expected_call);
        assert_eq!(plan.definition, expected_definition);
        assert!(plan.rewritten.contains("# ignored ))"));
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
            .expect("rewritten output remains parseable");
    }
}

#[test]
fn appends_extracted_definition_after_blank_line_when_input_has_trailing_newline() {
    let plan = plan_at(
        "(defun render () (+ 1 2))\n",
        &[0, 3],
        "render-sum",
        &[],
        false,
    );

    assert_eq!(
        plan.rewritten,
        "(defun render () (render-sum))\n\n(defun render-sum () (+ 1 2))\n"
    );
}

#[test]
fn plans_extract_function_before_anchor() {
    let input = "(defun first () value)\n(defun second () (+ x y))\n";
    let tree = SyntaxTree::parse(input).expect("parse fixture");
    let selection = tree
        .select_path(&Path::from_indexes(vec![1, 3]))
        .expect("select fixture");

    let plan = plan_extract_function(ExtractFunctionRequest {
        input,
        selection,
        path: Some(Path::from_indexes(vec![1, 3])),
        dialect: Dialect::CommonLisp,
        name: SymbolName::new("sum").expect("symbol fixture"),
        explicit_params: Vec::new(),
        infer_params: true,
        insert: ExtractFunctionInsert::Before,
        anchor_path: Some(Path::from_indexes(vec![0])),
    })
    .expect("plan extract function");

    assert!(plan.rewritten.starts_with("(defun sum (x y) (+ x y))\n\n"));
    assert!(plan.anchor_span.is_some());
}

#[test]
fn plans_extract_function_for_common_lisp_macrolet_body() {
    let plan = plan_at(
        "(defun render (outer input) (macrolet ((with-local (local) (list local outer))) (with-local input)))",
        &[0, 3],
        "build",
        &[],
        true,
    );

    assert_eq!(plan.call, "(build outer input)");
    assert_eq!(
        plan.definition,
        "(defun build (outer input) (macrolet ((with-local (local) (list local outer))) (with-local input)))"
    );
    assert_eq!(plan.inferred_params, vec!["outer", "input"]);
    SyntaxTree::parse(&plan.rewritten).expect("rewritten output remains parseable");
}

#[test]
fn plans_extract_function_for_common_lisp_symbol_macrolet_body() {
    let plan = plan_at(
        "(defun render (outer) (symbol-macrolet ((local (compute outer))) (list local outer)))",
        &[0, 3],
        "build",
        &[],
        true,
    );

    assert_eq!(plan.call, "(build outer)");
    assert_eq!(
        plan.definition,
        "(defun build (outer) (symbol-macrolet ((local (compute outer))) (list local outer)))"
    );
    assert_eq!(plan.inferred_params, vec!["outer"]);
    SyntaxTree::parse(&plan.rewritten).expect("rewritten output remains parseable");
}

#[test]
fn rejects_relative_extract_function_insertion_without_anchor_path() {
    let input = "(defun render () (+ x y))\n";
    let tree = SyntaxTree::parse(input).expect("parse fixture");
    let selection = tree
        .select_path(&Path::from_indexes(vec![0, 3]))
        .expect("select fixture");

    let error = plan_extract_function(ExtractFunctionRequest {
        input,
        selection,
        path: Some(Path::from_indexes(vec![0, 3])),
        dialect: Dialect::CommonLisp,
        name: SymbolName::new("sum").expect("symbol fixture"),
        explicit_params: Vec::new(),
        infer_params: true,
        insert: ExtractFunctionInsert::Before,
        anchor_path: None,
    })
    .expect_err("missing anchor path should be rejected");

    assert_eq!(
        error.to_string(),
        "--insert before/after requires --anchor-path"
    );
}

#[test]
fn rejects_selection_source_mismatches_without_panicking() {
    let source = "(defun f () α)";
    let tree = SyntaxTree::parse(source).expect("parse selection fixture");
    let path = Path::from_indexes(vec![0, 3]);
    let selection = tree.select_path(&path).expect("select fixture");

    for input in ["(defun g () β)", "(x)", "(defun f () aé)"] {
        let error = plan_extract_function(ExtractFunctionRequest {
            input,
            selection,
            path: Some(path.clone()),
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("extracted").expect("symbol fixture"),
            explicit_params: Vec::new(),
            infer_params: false,
            insert: ExtractFunctionInsert::Append,
            anchor_path: None,
        })
        .expect_err("mismatched selection source must be rejected");

        assert!(
            error
                .to_string()
                .contains("source used to build the selection")
        );
    }
}
