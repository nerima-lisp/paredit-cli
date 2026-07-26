use super::*;
use crate::domain::common_lisp::{
    CommonLispReaderLabelKind, common_lisp_reader_label_dispatches, common_lisp_reader_label_forms,
};

#[test]
fn detects_reader_label_definitions_and_references() {
    let input = "(let ((value #1=(cons :item #1#))) value)";
    let tree = SyntaxTree::parse(input).expect("parse succeeds");

    let dispatches = common_lisp_reader_label_dispatches(&tree);
    let forms = common_lisp_reader_label_forms(&tree);

    assert_eq!(
        dispatches
            .iter()
            .map(|dispatch| dispatch.kind)
            .collect::<Vec<_>>(),
        vec![
            CommonLispReaderLabelKind::Definition,
            CommonLispReaderLabelKind::Reference,
        ]
    );
    assert_eq!(
        forms
            .iter()
            .map(|form| form.span.slice(input))
            .collect::<Vec<_>>(),
        vec!["#1=(cons :item #1#)", "#1#"]
    );
}

#[test]
fn does_not_confuse_escaped_symbols_with_reader_labels() {
    let tree = SyntaxTree::parse("(|#1=| \\#2# #12x=)").expect("parse succeeds");

    assert!(common_lisp_reader_label_dispatches(&tree).is_empty());
}
