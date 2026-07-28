use crate::error::RenameError;
use crate::rename::domain::RenameReaderSafetyError;
use paredit_core_syntax::common_lisp::CommonLispReaderConditionalKind;
use paredit_core_syntax::dialect::{Dialect, SemanticOperation};
use paredit_core_syntax::sexpr::ByteOffset;
use paredit_core_syntax::sexpr::SymbolName;

#[test]
fn rejects_common_lisp_reader_conditionals_without_changing_input() {
    for (dispatch, expected_kind) in [
        ("#+", CommonLispReaderConditionalKind::Include),
        ("#-", CommonLispReaderConditionalKind::Exclude),
    ] {
        let input = format!("{dispatch}enabled (let ((value 1)) value)");
        let original = input.clone();

        let error = plan_rename_at(request(&input, "value 1", "count")).unwrap_err();

        // `downcast_ref` was the anyhow escape hatch; the typed error is
        // matchable directly, and nested to the exact reader-conditional kind.
        match &error {
            RenameError::RenameAt(RenameAtError::ReaderConditional(
                RenameReaderSafetyError::CommonLispReaderConditional { kind, .. },
            )) => assert_eq!(*kind, expected_kind),
            other => panic!("expected reader conditional error, got {other:?}"),
        }
        assert_eq!(input, original, "reader conditional: {dispatch}");
    }
}

#[test]
fn rejects_quoted_occurrence_without_fallback() {
    let input = "(let ((value 1)) 'value)";
    let error = plan_rename_at(request(input, "'value", "count")).unwrap_err();
    assert!(matches!(error, RenameError::RenameAt(_)), "{error:?}");
}

#[test]
fn rejects_read_eval_before_selection() {
    let input = "(defun render () #.(render))";
    let error = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(input.rfind("render").unwrap()),
        to: SymbolName::new("draw").unwrap(),
    })
    .unwrap_err();

    assert!(matches!(
        &error,
        RenameError::RenameAt(RenameAtError::ReaderConditional(
            RenameReaderSafetyError::CommonLispReadTimeEvaluation { .. }
        ))
    ));
}

#[test]
fn rejects_nested_quasiquote_occurrences() {
    let input = "(defun render () ``(,(render)))";
    let error = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(input.rfind("render").unwrap()),
        to: SymbolName::new("draw").unwrap(),
    })
    .unwrap_err();

    assert!(
        matches!(
            &error,
            RenameError::RenameAt(RenameAtError::InertReaderContext)
        ),
        "{error:?}"
    );
}

#[test]
fn rejects_utf8_mid_byte_offset() {
    let input = "(let ((café 1)) café)";
    let at = input.find("é").expect("non-ASCII symbol") + 1;
    let error = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("coffee").unwrap(),
    })
    .unwrap_err();
    assert!(
        matches!(
            &error,
            RenameError::RenameAt(RenameAtError::InvalidSelection)
        ),
        "{error:?}"
    );
}

#[test]
fn rejects_atom_end_and_out_of_range() {
    let input = "(let ((value 1)) value)";
    for at in [input.rfind("value").unwrap() + "value".len(), input.len()] {
        let error = plan_rename_at(RenameAtRequest {
            input,
            dialect: Dialect::CommonLisp,
            at: ByteOffset::new(at),
            to: SymbolName::new("count").unwrap(),
        })
        .unwrap_err();
        assert!(
            matches!(
                &error,
                RenameError::RenameAt(RenameAtError::InvalidSelection)
            ),
            "{error:?}"
        );
    }
}

#[test]
fn rejects_package_qualified_keyword_and_uninterned_symbols() {
    for symbol in ["pkg:foo", "pkg::foo", ":foo", "#:foo"] {
        let input = format!("(defun {symbol} () ({symbol}))");
        let error = plan_rename_at(RenameAtRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            at: ByteOffset::new(input.find(symbol).expect("symbol")),
            to: SymbolName::new("bar").unwrap(),
        })
        .unwrap_err();

        assert!(
            matches!(
                &error,
                RenameError::RenameAt(RenameAtError::UnsupportedPackageSyntax)
            ),
            "symbol: {symbol}"
        );
    }
}

#[test]
fn rejects_package_syntax_in_replacement_symbol() {
    let input = "(defun foo () (foo))";
    let error = plan_rename_at(request(input, "foo", "pkg:bar")).unwrap_err();

    assert!(
        matches!(
            &error,
            RenameError::RenameAt(RenameAtError::UnsupportedPackageSyntax)
        ),
        "{error:?}"
    );
}

#[test]
fn support_predicate_accepts_every_dialect_with_verified_rename_semantics() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Lfe,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Hy,
        Dialect::Carp,
        Dialect::Janet,
        Dialect::Fennel,
    ] {
        assert!(
            super::super::supports_rename_at_dialect(dialect),
            "{dialect:?}"
        );
        assert!(
            dialect.supports_semantic_operation(SemanticOperation::RenameBinding),
            "{dialect:?}"
        );
    }

    // Unknown input has no scope shapes at all, so it still fails closed.
    assert!(!super::super::supports_rename_at_dialect(Dialect::Unknown));
}

#[test]
fn resolves_lexical_bindings_in_every_supported_dialect() {
    // The offset points at the binding name in each fixture.
    let cases = [
        (
            Dialect::CommonLisp,
            "(let ((x 1)) (+ x x))",
            7,
            "(let ((y 1)) (+ y y))",
        ),
        (
            Dialect::EmacsLisp,
            "(let ((x 1)) (+ x x))",
            7,
            "(let ((y 1)) (+ y y))",
        ),
        (
            Dialect::Lfe,
            "(let ((x 1)) (+ x x))",
            7,
            "(let ((y 1)) (+ y y))",
        ),
        (
            Dialect::Scheme,
            "(let ((x 1)) (+ x x))",
            7,
            "(let ((y 1)) (+ y y))",
        ),
        (
            Dialect::Racket,
            "(let ((x 1)) (+ x x))",
            7,
            "(let ((y 1)) (+ y y))",
        ),
        (
            Dialect::Clojure,
            "(let [x 1] (+ x x))",
            6,
            "(let [y 1] (+ y y))",
        ),
        (Dialect::Hy, "(let [x 1] (+ x x))", 6, "(let [y 1] (+ y y))"),
        (
            Dialect::Carp,
            "(let [x 1] (+ x x))",
            6,
            "(let [y 1] (+ y y))",
        ),
        (
            Dialect::Janet,
            "(let [x 1] (+ x x))",
            6,
            "(let [y 1] (+ y y))",
        ),
        (
            Dialect::Fennel,
            "(let [x 1] (+ x x))",
            6,
            "(let [y 1] (+ y y))",
        ),
    ];

    for (dialect, input, at, expected) in cases {
        let plan = plan_rename_at(RenameAtRequest {
            input,
            dialect,
            at: ByteOffset::new(at),
            to: SymbolName::new("y").unwrap(),
        })
        .unwrap_or_else(|error| panic!("{dialect:?}: {error:?}"));

        assert_eq!(plan.rewritten, expected, "{dialect:?}");
    }
}

#[test]
fn leaves_the_call_head_alone_only_where_it_names_a_function() {
    // `(f 2)` calls the function `f` in a Lisp-2 no matter what `let` bound,
    // and is an ordinary reference to the binding in a Lisp-1.
    let cases = [
        (
            Dialect::EmacsLisp,
            "(let ((f 1)) (f 2) (g f))",
            7,
            "(let ((h 1)) (f 2) (g h))",
        ),
        (
            Dialect::Lfe,
            "(let ((f 1)) (f 2) (g f))",
            7,
            "(let ((h 1)) (f 2) (g h))",
        ),
        (
            Dialect::Scheme,
            "(let ((f 1)) (f 2) (g f))",
            7,
            "(let ((h 1)) (h 2) (g h))",
        ),
        (
            Dialect::Fennel,
            "(let [f 1] (f 2) (g f))",
            6,
            "(let [h 1] (h 2) (g h))",
        ),
    ];

    for (dialect, input, at, expected) in cases {
        let plan = plan_rename_at(RenameAtRequest {
            input,
            dialect,
            at: ByteOffset::new(at),
            to: SymbolName::new("h").unwrap(),
        })
        .unwrap_or_else(|error| panic!("{dialect:?}: {error:?}"));

        assert_eq!(plan.rewritten, expected, "{dialect:?}");
    }
}

#[test]
fn rejects_unsupported_dialects_before_parsing_malformed_input() {
    // The gate has to come first: the input below does not parse, so reaching
    // a parse error would prove the dialect was never checked.
    let error = plan_rename_at(RenameAtRequest {
        input: "(",
        dialect: Dialect::Unknown,
        at: ByteOffset::new(0),
        to: SymbolName::new("bar").unwrap(),
    })
    .unwrap_err();

    assert!(
        matches!(
            &error,
            RenameError::RenameAt(RenameAtError::UnsupportedDialect)
        ),
        "{error:?}"
    );
}
use super::*;
