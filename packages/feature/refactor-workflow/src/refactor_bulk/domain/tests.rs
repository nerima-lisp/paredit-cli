use super::*;

fn request(input: &str, operations: serde_json::Value) -> BulkRequest {
    serde_json::from_value(serde_json::json!({
        "version": 1, "input_hash": stable_text_hash(input), "operations": operations,
    }))
    .expect("typed request")
}

fn plan(input: &str, operations: serde_json::Value) -> CliResult<BulkPlan> {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("original tree");
    plan_bulk(
        input,
        &tree,
        Dialect::CommonLisp,
        request(input, operations),
    )
}

#[test]
fn mixed_operations_use_original_paths_and_preserve_trivia() {
    let input = "; heading\r\n(foo bar)  ; keep\r\n(baz foo)\r\n";
    let result = plan(
        input,
        serde_json::json!([
            {"kind":"wrap", "path":"0", "delimiter":"paren"},
            {"kind":"rename-in-form", "path":"1", "from":"foo", "to":"qux"},
        ]),
    )
    .expect("disjoint plan");
    assert_eq!(
        result.rewritten,
        "; heading\r\n((foo bar))  ; keep\r\n(baz qux)\r\n"
    );
    assert_eq!(result.tree.source(), result.rewritten);
    assert_eq!(result.edit_count, 2);
    assert_eq!(result.tree.root_view().children.len(), 2);
}

#[test]
fn rename_does_not_see_a_previous_operations_replacement() {
    let result = plan(
        "(foo)",
        serde_json::json!([
            {"kind":"rename-in-form", "path":"0", "from":"foo", "to":"bar"},
            {"kind":"rename-in-form", "path":"0", "from":"bar", "to":"baz"},
        ]),
    )
    .expect("original snapshot");
    assert_eq!(result.rewritten, "(bar)");
    assert_eq!(result.edit_count, 1);
}

#[test]
fn actual_disjoint_spans_allow_overlapping_selected_scopes() {
    let result = plan(
        "(foo (bar))",
        serde_json::json!([
            {"kind":"wrap", "path":"0.1", "delimiter":"paren"},
            {"kind":"rename-in-form", "path":"0", "from":"foo", "to":"qux"},
        ]),
    )
    .expect("disjoint actual edits");
    assert_eq!(result.rewritten, "(qux ((bar)))");
}

#[test]
fn actual_overlap_is_refused_in_both_orders() {
    let wrap = serde_json::json!({"kind":"wrap","path":"0","delimiter":"paren"});
    let rename = serde_json::json!({"kind":"rename-in-form","path":"0","from":"foo","to":"bar"});
    for operations in [
        serde_json::json!([wrap, rename]),
        serde_json::json!([rename, wrap]),
    ] {
        assert!(
            plan("(foo)", operations)
                .unwrap_err()
                .to_string()
                .contains("overlapping")
        );
    }
}

#[test]
fn invalid_output_is_refused() {
    assert!(
        plan(
            "(foo)",
            serde_json::json!([
                {"kind":"rename-in-form","path":"0","from":"foo","to":"|"},
            ])
        )
        .unwrap_err()
        .to_string()
        .contains("does not reparse")
    );
}

#[test]
fn guards_reject_stale_unknown_dialect_and_foreign_tree() {
    let input = "(foo)";
    let operations = serde_json::json!([{"kind":"wrap","path":"0","delimiter":"paren"}]);
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("tree");
    let mut stale = request(input, operations.clone());
    stale.input_hash = stable_text_hash("(bar)");
    assert!(plan_bulk(input, &tree, Dialect::CommonLisp, stale).is_err());
    assert!(
        plan_bulk(
            input,
            &tree,
            Dialect::Unknown,
            request(input, operations.clone())
        )
        .is_err()
    );
    let other = SyntaxTree::parse_with_dialect("(bar)", Dialect::CommonLisp).expect("foreign tree");
    assert!(
        plan_bulk(
            input,
            &other,
            Dialect::CommonLisp,
            request(input, operations)
        )
        .is_err()
    );
}

#[test]
fn strict_json_refuses_unknown_duplicate_and_unsupported_fields() {
    for text in [
        r#"{"version":1,"version":1,"input_hash":"x","operations":[]}"#,
        r#"{"version":1,"input_hash":"x","operations":[],"extra":true}"#,
        r#"{"version":1,"input_hash":"x","operations":[{"kind":"kill","path":"0"}]}"#,
        r#"{"version":1,"input_hash":"x","operations":[{"kind":"wrap","path":"0","path":"1","delimiter":"paren"}]}"#,
        r#"{"version":1,"input_hash":"x","operations":[{"kind":"wrap","path":"0","delimiter":"paren","extra":true}]}"#,
    ] {
        assert!(serde_json::from_str::<BulkRequest>(text).is_err(), "{text}");
    }
}

#[test]
fn version_empty_hash_root_and_unreachable_path_are_refused() {
    let input = "(foo)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("tree");
    let operations = serde_json::json!([{"kind":"wrap","path":"0","delimiter":"paren"}]);
    let mut version = request(input, operations.clone());
    version.version = 2;
    assert!(plan_bulk(input, &tree, Dialect::CommonLisp, version).is_err());
    let mut hash = request(input, operations);
    hash.input_hash = "invalid".into();
    assert!(plan_bulk(input, &tree, Dialect::CommonLisp, hash).is_err());
    assert!(plan(input, serde_json::json!([])).is_err());
    for path in ["", "4", "not-a-path"] {
        assert!(
            plan(
                input,
                serde_json::json!([{"kind":"wrap","path":path,"delimiter":"paren"}])
            )
            .is_err()
        );
    }
}

#[test]
fn rename_names_must_remain_single_unprefixed_atoms() {
    for name in ["baz\\", "|", "'bar"] {
        for (from, to) in [("foo", name), (name, "bar")] {
            let error = plan(
                "(foo arg)",
                serde_json::json!([
                    {"kind":"rename-in-form","path":"0","from":from,"to":to}
                ]),
            )
            .unwrap_err();
            assert!(error.to_string().contains("single unprefixed atom"));
        }
    }
}

#[test]
fn valid_escaped_rename_names_preserve_the_adjacent_atom() {
    for name in ["|Baz|", "baz\\\\", "baz\\q"] {
        let result = plan(
            "(foo arg)",
            serde_json::json!([
                {"kind":"rename-in-form","path":"0","from":"foo","to":name}
            ]),
        )
        .expect("complete escaped symbol");
        assert_eq!(result.rewritten, format!("({name} arg)"));
        let root = result.tree.root_view();
        assert_eq!(root.children[0].children.len(), 2);
        assert_eq!(root.children[0].children[1].text.as_deref(), Some("arg"));
    }
}

#[test]
fn rename_matching_respects_the_source_dialect() {
    let input = "(foo Foo |Foo|)";
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Clojure,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("source tree");
        let result = plan_bulk(
            input,
            &tree,
            dialect,
            request(
                input,
                serde_json::json!([
                    {"kind":"rename-in-form","path":"0","from":"foo","to":"bar"}
                ]),
            ),
        )
        .expect("dialect-aware rename");
        let expected = if dialect == Dialect::CommonLisp {
            "(bar bar |Foo|)"
        } else {
            "(bar Foo |Foo|)"
        };
        assert_eq!(result.rewritten, expected, "{dialect:?}");
    }
}
