use proptest::prelude::*;

use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::form_shape::FormShape;

#[test]
fn plans_multiple_replacements_in_reverse_span_order() {
    let input = "(foo 1)\n(foo 2)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).unwrap();

    let plan = plan_replace_forms(ReplaceFormsRequest {
        input,
        tree: &tree,
        dialect: Dialect::CommonLisp,
        paths: vec![Path::from_indexes(vec![0]), Path::from_indexes(vec![1])],
        replacement: "(bar 0)",
        require_same_shape: true,
    })
    .unwrap();

    assert!(plan.changed);
    assert_eq!(plan.targets.len(), 2);
    assert_eq!(plan.rewritten, "(bar 0)\n(bar 0)\n");
    assert_eq!(
        plan.original_shape.as_ref().map(FormShape::as_str),
        Some("(paren head:foo _atom)")
    );
    assert_eq!(plan.replacement_shape.as_str(), "(paren head:bar _atom)");
}

#[test]
fn rejects_duplicate_paths() {
    let input = "(foo 1)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).unwrap();

    let error = plan_replace_forms(ReplaceFormsRequest {
        input,
        tree: &tree,
        dialect: Dialect::CommonLisp,
        paths: vec![Path::from_indexes(vec![0]), Path::from_indexes(vec![0])],
        replacement: "(bar 0)",
        require_same_shape: false,
    })
    .unwrap_err();

    assert!(error.to_string().contains("duplicate --path: 0"));
}

#[test]
fn rejects_shape_mismatch_when_required() {
    let input = "(foo 1)\n(foo 1 2)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).unwrap();

    let error = plan_replace_forms(ReplaceFormsRequest {
        input,
        tree: &tree,
        dialect: Dialect::CommonLisp,
        paths: vec![Path::from_indexes(vec![0]), Path::from_indexes(vec![1])],
        replacement: "(bar 0)",
        require_same_shape: true,
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("expected all selected forms to share shape")
    );
}

#[test]
fn rejects_input_that_does_not_match_tree_source() {
    let tree = SyntaxTree::parse_with_dialect("(a x)", Dialect::CommonLisp).unwrap();

    let error = plan_replace_forms(ReplaceFormsRequest {
        input: "(é x)",
        tree: &tree,
        dialect: Dialect::CommonLisp,
        paths: vec![Path::from_indexes(vec![0, 1])],
        replacement: "y",
        require_same_shape: false,
    })
    .unwrap_err();

    assert!(error.to_string().contains("does not match"));
}

#[test]
fn supports_each_known_dialect_with_its_reader_semantics() {
    let cases = [
        (Dialect::CommonLisp, r"(list #\))", r"#\(", r"(list #\()"),
        (Dialect::EmacsLisp, r"(list ?\))", r"?\(", r"(list ?\()"),
        (Dialect::Scheme, "(list old)", "new", "(list new)"),
        (
            Dialect::Clojure,
            r#"(vector #inst "2020-01-01")"#,
            r#"#uuid "00000000-0000-0000-0000-000000000000""#,
            r#"(vector #uuid "00000000-0000-0000-0000-000000000000")"#,
        ),
        (Dialect::Janet, "(list old)", "new", "(list new)"),
        (Dialect::Fennel, "(list old)", "new", "(list new)"),
    ];

    for (dialect, input, replacement, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).unwrap();
        let plan = plan_replace_forms(ReplaceFormsRequest {
            input,
            tree: &tree,
            dialect,
            paths: vec![Path::from_indexes(vec![0, 1])],
            replacement,
            require_same_shape: false,
        })
        .unwrap();

        assert_eq!(plan.rewritten, expected, "dialect: {dialect:?}");
        assert!(
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).is_ok(),
            "dialect: {dialect:?}"
        );
    }
}

#[test]
fn dialect_specific_reader_semantics_do_not_leak() {
    assert!(SyntaxTree::parse_with_dialect(r"(list #\))", Dialect::Clojure).is_err());
    assert!(SyntaxTree::parse_with_dialect(r"[?\)]", Dialect::EmacsLisp).is_ok());
    assert!(SyntaxTree::parse_with_dialect(r"[?\)]", Dialect::CommonLisp).is_err());
    assert!(
        SyntaxTree::parse_with_dialect(r#"(vector #inst "2020-01-01")"#, Dialect::CommonLisp)
            .is_err()
    );
}

#[test]
fn rejects_unknown_dialect_before_parsing_malformed_documents() {
    let tree = SyntaxTree::parse_with_dialect("placeholder", Dialect::CommonLisp).unwrap();

    let error = plan_replace_forms(ReplaceFormsRequest {
        input: ")",
        tree: &tree,
        dialect: Dialect::Unknown,
        paths: vec![Path::from_indexes(vec![0])],
        replacement: "(",
        require_same_shape: false,
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "replace-forms requires a known dialect");
}

fn lisp_symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_map(|name| name)
}

proptest! {
    #[test]
    fn pbt_replacing_same_shape_forms_keeps_output_parseable(
        left in lisp_symbol_strategy(),
        right in lisp_symbol_strategy(),
        replacement in lisp_symbol_strategy(),
    ) {
        let input = format!("({left} 1)\n({right} 2)\n");
        let tree = SyntaxTree::parse_with_dialect(&input, Dialect::CommonLisp).unwrap();
        let replacement_text = format!("({replacement} 0)");

        let plan = plan_replace_forms(ReplaceFormsRequest {
            input: &input,
            tree: &tree,
            dialect: Dialect::CommonLisp,
            paths: vec![Path::from_indexes(vec![0]), Path::from_indexes(vec![1])],
            replacement: &replacement_text,
            require_same_shape: false,
        })
        .unwrap();

        prop_assert!(plan.changed);
        prop_assert_eq!(plan.targets.len(), 2);
        prop_assert_eq!(&plan.rewritten, &format!("{replacement_text}\n{replacement_text}\n"));
        prop_assert!(
            SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp).is_ok()
        );
    }
}
