//! What the typed errors buy over `anyhow::Error`.
//!
//! Every assertion here was impossible before §9.2: the only thing a caller
//! could observe about a failure was its rendered message. These tests pin the
//! two things that replaced message-reading — the *kind* of failure, and the
//! fact that the wording still matches exactly.

use super::*;
use crate::sexpr::error::{SelectionError, SexprError, SpanError, StructureError};

/// A refusal about tree shape is a `Structure`, not a `Selection`.
///
/// The distinction is the whole point: a caller can offer "try a different
/// selection" for the first and must not for the second.
#[test]
fn a_shape_refusal_is_structural() {
    let input = "alpha";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");

    assert_eq!(
        Edit::raise(input, &tree, selection).unwrap_err(),
        SexprError::Structure(StructureError::RaiseTopLevel)
    );
}

/// A selection from another tree is a `Selection`, and carries no operation
/// prefix — only a source mismatch does.
#[test]
fn a_foreign_tree_is_a_selection_failure_and_is_not_prefixed() {
    let source = "(alpha beta)";
    let first = SyntaxTree::parse(source).expect("valid");
    let second = SyntaxTree::parse(source).expect("valid");
    let selection = first.select_path(&parse_path("0.1")).expect("selection");

    let error = Edit::kill(source, &second, selection).unwrap_err();
    assert_eq!(error, SexprError::Selection(SelectionError::TreeMismatch));
    assert_eq!(
        error.to_string(),
        "selection belongs to a different syntax tree"
    );
}

/// A source mismatch *is* prefixed, and the prefix is now a variant rather
/// than `error.to_string().starts_with("input ")`.
#[test]
fn a_source_mismatch_names_the_operation() {
    let source = "(alpha beta)";
    let tree = SyntaxTree::parse(source).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");

    for error in [
        Edit::replace("(alpha zeta)", selection, "delta").unwrap_err(),
        Edit::kill("(alpha zeta)", &tree, selection).unwrap_err(),
    ] {
        assert_eq!(
            error,
            SexprError::EditSelection {
                source: SelectionError::SourceMismatch,
            }
        );
        assert_eq!(
            error.to_string(),
            "edit input does not match the source used to build the selection"
        );
        assert_eq!(
            error.root_cause().to_string(),
            "input does not match the source used to build the selection"
        );
    }
}

/// A corrupted span reaches the caller as a `SpanError` nested two deep,
/// rather than as a flattened string.
#[test]
fn a_corrupted_span_keeps_its_cause() {
    let source = "(alpha beta)";
    let mut tree = SyntaxTree::parse(source).expect("valid");
    let selected_id = tree
        .select_path(&parse_path("0.1"))
        .expect("selection")
        .node_id;
    tree.nodes[selected_id.get()].span = ByteSpan::new(ByteOffset::new(9), ByteOffset::new(7));
    let selection = Selection {
        tree: &tree,
        node_id: selected_id,
    };

    let error = Edit::replace(source, selection, "delta").unwrap_err();
    assert_eq!(
        error,
        SexprError::EditSelection {
            source: SelectionError::InvalidSpan {
                source: SpanError::StartExceedsEnd { start: 9, end: 7 },
            },
        }
    );
    assert_eq!(error.root_cause().to_string(), "span start 9 exceeds end 7");
}

/// A path that does not resolve reports which segment failed, as data.
#[test]
fn an_out_of_range_path_reports_the_segment() {
    let tree = SyntaxTree::parse("(alpha beta)").expect("valid");

    let error = tree.select_path(&parse_path("0.9")).unwrap_err();
    let SexprError::Selection(SelectionError::PathSegmentOutOfRange { segment, detail }) = error
    else {
        panic!("expected an out-of-range path segment, got {error:?}");
    };
    assert_eq!(segment, 9);
    assert_eq!(
        detail,
        "the form at path 0 has 2 child expressions (valid indexes 0..=1)"
    );
}

/// `SymbolName` and `ExpressionPath` fail with their own types, because they
/// parse user input rather than inspect a tree.
#[test]
fn from_str_failures_are_their_own_types() {
    use crate::sexpr::error::{PathError, SymbolError};

    assert_eq!("".parse::<SymbolName>().unwrap_err(), SymbolError::Empty);
    assert_eq!(
        "a b".parse::<SymbolName>().unwrap_err(),
        SymbolError::ReaderDelimiterOrWhitespace {
            value: "a b".to_owned(),
        }
    );
    assert_eq!(
        "0.x".parse::<ExpressionPath>().unwrap_err(),
        PathError::InvalidSegment {
            segment: "x".to_owned(),
        }
    );
}
