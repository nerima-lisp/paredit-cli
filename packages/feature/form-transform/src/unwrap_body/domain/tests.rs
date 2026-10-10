use super::*;

fn unwrap(
    input: &str,
    dialect: Dialect,
    indexes: &[usize],
    body_start: usize,
) -> FormTransformResult<Plan> {
    plan(Request {
        input,
        dialect,
        path: Path::from_indexes(indexes.to_vec()),
        expected_function: None,
        body_start,
    })
}

#[test]
fn retains_all_body_forms_and_comments() {
    let input = "(wrap config ; body\n  (a) ; middle\n  (b) ; trailing\n)\n(after)";
    let result = unwrap(input, Dialect::CommonLisp, &[0], 1).unwrap();
    assert_eq!(
        result.replacement,
        "; body\n  (a) ; middle\n  (b) ; trailing\n"
    );
    assert_eq!(
        result.rewritten,
        "; body\n  (a) ; middle\n  (b) ; trailing\n\n(after)"
    );
    assert_eq!(result.body_count, 2);
    assert_eq!(result.body_span.slice(input), result.replacement);
}

#[test]
fn top_level_supports_known_dialects() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Lfe,
        Dialect::Hy,
        Dialect::Carp,
        Dialect::Janet,
        Dialect::Fennel,
    ] {
        assert_eq!(
            unwrap("(wrap config (a) (b))", dialect, &[0], 1)
                .unwrap()
                .rewritten,
            "(a) (b)"
        );
    }
}

#[test]
fn expands_only_supported_sequence_arguments() {
    for (dialect, head) in [
        (Dialect::CommonLisp, "progn"),
        (Dialect::CommonLisp, "CL:PROGN"),
        (Dialect::CommonLisp, "COMMON-LISP:PROGN"),
        (Dialect::EmacsLisp, "progn"),
        (Dialect::Scheme, "begin"),
        (Dialect::Racket, "begin"),
        (Dialect::Clojure, "do"),
    ] {
        let input = format!("({head} (before) (wrap (a) (b)) (after))");
        assert_eq!(
            unwrap(&input, dialect, &[0, 2], 0).unwrap().rewritten,
            format!("({head} (before) (a) (b) (after))")
        );
    }
    for (dialect, head) in [
        (Dialect::CommonLisp, "OTHER:PROGN"),
        (Dialect::Scheme, "progn"),
        (Dialect::Clojure, "begin"),
        (Dialect::EmacsLisp, "consume"),
    ] {
        let input = format!("({head} (wrap (a) (b)))");
        assert!(matches!(
            unwrap(&input, dialect, &[0, 1], 0),
            Err(crate::error::FormTransformError::Target(
                TransformTargetError::UnwrapBodyNotASequence
            ))
        ));
    }
    assert!(unwrap("((wrap a b) x)", Dialect::CommonLisp, &[0, 0], 0).is_err());
}

#[test]
fn single_body_is_allowed_in_an_expression() {
    assert_eq!(
        unwrap(
            "(consume (wrap config (a)))",
            Dialect::CommonLisp,
            &[0, 1],
            1
        )
        .unwrap()
        .rewritten,
        "(consume (a))"
    );
}

#[test]
fn rejects_quoted_and_prefixed_wrappers_and_ancestors() {
    for input in [
        "'(wrap a)",
        "`(wrap a)",
        "#'(wrap a)",
        "#.(wrap a)",
        "#2A((wrap a))",
        "#C(1 2)",
    ] {
        assert!(
            unwrap(input, Dialect::CommonLisp, &[0], 0).is_err(),
            "{input}"
        );
    }
    for (input, path) in [
        ("'(progn (wrap a))", vec![0, 1]),
        ("`(progn (wrap a))", vec![0, 1]),
        ("#'(progn (wrap a))", vec![0, 1]),
        ("(quote (wrap a))", vec![0, 1]),
        ("(quasiquote (wrap a))", vec![0, 1]),
        ("(CL:FUNCTION (wrap a))", vec![0, 1]),
        ("('wrap a)", vec![0]),
    ] {
        assert!(
            unwrap(input, Dialect::CommonLisp, &path, 0).is_err(),
            "{input}"
        );
    }
    assert!(unwrap("(quote a)", Dialect::CommonLisp, &[0], 0).is_err());
}

#[test]
fn retains_quoted_body_without_unquoting_it() {
    assert_eq!(
        unwrap("(wrap '(a b))", Dialect::CommonLisp, &[0], 0)
            .unwrap()
            .replacement,
        "'(a b)"
    );
    assert_eq!(
        unwrap("quote (wrap a)", Dialect::CommonLisp, &[1], 0)
            .unwrap()
            .rewritten,
        "quote a"
    );
}

#[test]
fn rejects_missing_body_invalid_target_and_unknown_dialect() {
    for (input, start) in [
        ("(wrap)", 0),
        ("(wrap a)", 1),
        ("(wrap a)", usize::MAX),
        ("atom", 0),
        ("((f) a)", 0),
    ] {
        assert!(unwrap(input, Dialect::CommonLisp, &[0], start).is_err());
    }
    assert!(unwrap("(wrap a)", Dialect::Unknown, &[0], 0).is_err());
    assert!(unwrap("(wrap a)", Dialect::CommonLisp, &[8], 0).is_err());
}

#[test]
fn function_guard_is_dialect_aware() {
    let request = |dialect, guard: &str| Request {
        input: "(WRAP a)",
        dialect,
        path: Path::from_indexes(vec![0]),
        expected_function: Some(SymbolName::new(guard).unwrap()),
        body_start: 0,
    };
    assert!(plan(request(Dialect::CommonLisp, "wrap")).is_ok());
    assert!(plan(request(Dialect::EmacsLisp, "wrap")).is_err());
    assert!(plan(request(Dialect::CommonLisp, "other")).is_err());
}

#[test]
fn refuses_comment_loss_and_boundary_fusion() {
    assert!(unwrap("(wrap ; lost\n config a)", Dialect::CommonLisp, &[0], 1).is_err());
    for input in ["(wrap a b)tail", "head(wrap a b)", "(wrap a b)\"tail\""] {
        assert!(
            unwrap(
                input,
                Dialect::CommonLisp,
                &[if input.starts_with("head") { 1 } else { 0 }],
                0
            )
            .is_err(),
            "{input}"
        );
    }
    assert!(unwrap("(wrap a ; retained\n)tail", Dialect::CommonLisp, &[0], 0).is_ok());
}
