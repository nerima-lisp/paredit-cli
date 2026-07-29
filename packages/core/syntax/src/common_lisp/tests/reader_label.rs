use super::*;
use crate::common_lisp::{
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

/// A dialect-aware Common Lisp parse consumes `#n=(…)` into one opaque atom,
/// so the query cannot assume the labelled datum is a sibling node. Missing
/// this made every reader label invisible on the path the CLI actually takes,
/// while the legacy-parse tests above stayed green.
#[test]
fn finds_labels_when_the_dialect_aware_parse_makes_the_form_opaque() {
    let input = "(list #1=(a b) #1#)";
    let tree =
        SyntaxTree::parse_with_dialect(input, crate::dialect::Dialect::CommonLisp).expect("parse");

    let forms = common_lisp_reader_label_forms(&tree);
    assert_eq!(
        forms
            .iter()
            .map(|form| (form.kind, form.span.slice(input)))
            .collect::<Vec<_>>(),
        vec![
            (CommonLispReaderLabelKind::Definition, "#1=(a b)"),
            (CommonLispReaderLabelKind::Reference, "#1#"),
        ]
    );
}

/// `'#1=(…)` scans as one atom whose `symbol_offset` is zero, so the quote is
/// still in the text the query reads.
#[test]
fn finds_a_label_behind_a_quote_prefix() {
    let input = "(defvar *x* '#1=(a b))";
    let tree =
        SyntaxTree::parse_with_dialect(input, crate::dialect::Dialect::CommonLisp).expect("parse");

    let forms = common_lisp_reader_label_forms(&tree);
    assert_eq!(forms.len(), 1, "{forms:?}");
    assert_eq!(forms[0].kind, CommonLispReaderLabelKind::Definition);
    assert_eq!(forms[0].dispatch_span.slice(input), "#1=");
}

/// A multi-digit label's dispatch is four bytes, not three. Assuming a fixed
/// width would put the span boundary inside the number.
#[test]
fn a_multi_digit_label_reports_its_whole_dispatch() {
    let input = "(list #12=(a) #12#)";
    let tree =
        SyntaxTree::parse_with_dialect(input, crate::dialect::Dialect::CommonLisp).expect("parse");

    let forms = common_lisp_reader_label_forms(&tree);
    assert_eq!(forms[0].dispatch_span.slice(input), "#12=");
    assert_eq!(forms[1].dispatch_span.slice(input), "#12#");
}
