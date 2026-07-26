use super::*;
use crate::domain::common_lisp::{CommonLispReaderLiteralKind, common_lisp_reader_literals};

#[test]
fn detects_common_lisp_vector_literals_in_source_order() {
    let input = "#(first #(nested)) (call value) #(last)";
    let tree = SyntaxTree::parse(input).expect("parse succeeds");

    let literals = common_lisp_reader_literals(&tree);

    assert_eq!(
        literals
            .iter()
            .map(|literal| literal.kind)
            .collect::<Vec<_>>(),
        vec![
            CommonLispReaderLiteralKind::Vector,
            CommonLispReaderLiteralKind::Vector,
            CommonLispReaderLiteralKind::Vector,
        ]
    );
    assert_eq!(
        literals
            .iter()
            .map(|literal| literal.span.slice(input))
            .collect::<Vec<_>>(),
        vec!["#(first #(nested))", "#(nested)", "#(last)"]
    );
}
