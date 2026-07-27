use crate::domain::common_lisp::CommonLispReaderConditionalKind;
use crate::domain::rename::RenameReaderSafetyError;

#[test]
fn rejects_common_lisp_reader_conditionals_without_changing_input() {
    for (dispatch, expected_kind) in [
        ("#+", CommonLispReaderConditionalKind::Include),
        ("#-", CommonLispReaderConditionalKind::Exclude),
    ] {
        let input = format!("{dispatch}enabled (let ((value 1)) value)");
        let original = input.clone();

        let error = plan_rename_at(request(&input, "value 1", "count")).unwrap_err();

        match error.downcast_ref::<RenameAtError>() {
            Some(RenameAtError::ReaderConditional(
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
    assert!(error.downcast_ref::<RenameAtError>().is_some());
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
        error.downcast_ref::<RenameAtError>(),
        Some(RenameAtError::ReaderConditional(
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

    assert_eq!(
        error.downcast_ref::<RenameAtError>(),
        Some(&RenameAtError::InertReaderContext)
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
    assert_eq!(
        error.downcast_ref::<RenameAtError>(),
        Some(&RenameAtError::InvalidSelection)
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
        assert_eq!(
            error.downcast_ref::<RenameAtError>(),
            Some(&RenameAtError::InvalidSelection)
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

        assert_eq!(
            error.downcast_ref::<RenameAtError>(),
            Some(&RenameAtError::UnsupportedPackageSyntax),
            "symbol: {symbol}"
        );
    }
}

#[test]
fn rejects_package_syntax_in_replacement_symbol() {
    let input = "(defun foo () (foo))";
    let error = plan_rename_at(request(input, "foo", "pkg:bar")).unwrap_err();

    assert_eq!(
        error.downcast_ref::<RenameAtError>(),
        Some(&RenameAtError::UnsupportedPackageSyntax)
    );
}

#[test]
fn support_predicate_accepts_only_common_lisp() {
    assert!(super::super::supports_rename_at_dialect(
        Dialect::CommonLisp
    ));

    for dialect in [
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ] {
        assert!(!super::super::supports_rename_at_dialect(dialect));
    }
}

#[test]
fn rejects_unsupported_dialects_before_parsing_malformed_input() {
    for dialect in [
        Dialect::EmacsLisp,
        Dialect::Scheme,
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

        assert_eq!(
            error.downcast_ref::<RenameAtError>(),
            Some(&RenameAtError::UnsupportedDialect)
        );
    }
}
use super::*;
