use super::*;
use crate::domain::common_lisp::{
    CommonLispReaderConditionalKind, common_lisp_reader_conditional_dispatches,
    common_lisp_reader_conditional_forms,
};

#[test]
fn detects_bare_include_and_exclude_dispatches() {
    let tree = SyntaxTree::parse("#+ #-").expect("parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(
        dispatches
            .iter()
            .map(|dispatch| dispatch.kind)
            .collect::<Vec<_>>(),
        vec![
            CommonLispReaderConditionalKind::Include,
            CommonLispReaderConditionalKind::Exclude,
        ]
    );
    assert_eq!(
        dispatches
            .iter()
            .map(|dispatch| dispatch.path.to_string())
            .collect::<Vec<_>>(),
        vec!["0", "1"]
    );
}

#[test]
fn detects_simple_and_compound_feature_conditions() {
    let tree = SyntaxTree::parse("#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)")
        .expect("parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].kind, CommonLispReaderConditionalKind::Include);
    assert_eq!(dispatches[1].kind, CommonLispReaderConditionalKind::Exclude);
    assert_eq!(
        dispatches[0]
            .span
            .slice("#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)"),
        "#+"
    );
    assert_eq!(
        dispatches[1]
            .span
            .slice("#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)"),
        "#-"
    );
}

#[test]
fn detects_dispatches_with_quote_and_function_reader_prefixes() {
    let tree = SyntaxTree::parse("'#+sbcl selected #'#-sbcl rejected").expect("parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].kind, CommonLispReaderConditionalKind::Include);
    assert_eq!(dispatches[1].kind, CommonLispReaderConditionalKind::Exclude);
    assert_eq!(
        dispatches[0]
            .span
            .slice("'#+sbcl selected #'#-sbcl rejected"),
        "#+"
    );
    assert_eq!(
        dispatches[1]
            .span
            .slice("'#+sbcl selected #'#-sbcl rejected"),
        "#-"
    );
}

#[test]
fn detects_multiple_and_nested_dispatches() {
    let tree = SyntaxTree::parse("(progn #+outer (progn #-inner skipped) #-fallback (run))")
        .expect("parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(
        dispatches
            .iter()
            .map(|dispatch| dispatch.kind)
            .collect::<Vec<_>>(),
        vec![
            CommonLispReaderConditionalKind::Include,
            CommonLispReaderConditionalKind::Exclude,
            CommonLispReaderConditionalKind::Exclude,
        ]
    );
    assert_eq!(
        dispatches
            .iter()
            .map(|dispatch| dispatch.path.to_string())
            .collect::<Vec<_>>(),
        vec!["0.1", "0.3.1", "0.4"]
    );
}

#[test]
fn does_not_confuse_clojure_conditionals_or_reader_comments_with_common_lisp_dispatches() {
    for input in ["#?(:clj selected :cljs ignored)", "#; #+sbcl ignored"] {
        let tree = SyntaxTree::parse(input).expect("parse succeeds");

        assert!(common_lisp_reader_conditional_dispatches(&tree).is_empty());
    }
}

#[test]
fn collects_complete_forms_from_legacy_and_dialect_aware_trees() {
    let input = "#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)";
    let legacy = SyntaxTree::parse(input).expect("legacy parse succeeds");
    let dialect_aware = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
        .expect("Common Lisp parse succeeds");

    for tree in [&legacy, &dialect_aware] {
        let forms = common_lisp_reader_conditional_forms(tree);

        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].kind, CommonLispReaderConditionalKind::Include);
        assert_eq!(forms[1].kind, CommonLispReaderConditionalKind::Exclude);
        assert_eq!(forms[0].dispatch_span.slice(input), "#+");
        assert_eq!(forms[1].dispatch_span.slice(input), "#-");
        assert_eq!(forms[0].span.slice(input), "#+sbcl (compile-file source)");
        assert_eq!(
            forms[1].span.slice(input),
            "#-(and sbcl x86-64) (load source)"
        );
    }
}

#[test]
fn reports_dispatch_spans_from_dialect_aware_opaque_forms() {
    let input = "#+sbcl selected #-sbcl rejected";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
        .expect("Common Lisp parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].span.slice(input), "#+");
    assert_eq!(dispatches[1].span.slice(input), "#-");
}
