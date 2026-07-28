use crate::error::RenameError;
use crate::rename::domain::RenameReaderSafetyError;
use paredit_core_syntax::common_lisp::CommonLispReaderConditionalKind;
use paredit_core_syntax::dialect::Dialect;
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
fn support_predicate_accepts_the_dialects_with_a_binding_table() {
    // The three dialects `build_binding_table` analyses and
    // `verify_rename_binding` accepts. Every other one has no way to prove
    // which occurrences belong to the selected binding.
    for dialect in [Dialect::CommonLisp, Dialect::Scheme, Dialect::Racket] {
        assert!(
            super::super::supports_rename_at_dialect(dialect),
            "{dialect:?}"
        );
    }

    for dialect in [
        Dialect::EmacsLisp,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ] {
        assert!(
            !super::super::supports_rename_at_dialect(dialect),
            "{dialect:?}"
        );
    }
}

#[test]
fn rejects_unsupported_dialects_before_parsing_malformed_input() {
    for dialect in [
        Dialect::EmacsLisp,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ] {
        let error = plan_rename_at(RenameAtRequest {
            input: "(",
            dialect,
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
}
use super::*;
