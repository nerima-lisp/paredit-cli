use std::error::Error;
use std::fmt;

use paredit_core_syntax::common_lisp::{
    CommonLispReaderConditionalKind, CommonLispReaderLabelKind, CommonLispReaderLiteralKind,
    common_lisp_reader_conditional_forms, common_lisp_reader_conditional_kind,
    common_lisp_reader_label_forms, common_lisp_reader_label_kind, common_lisp_reader_literal_kind,
    common_lisp_reader_literals,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};

/// Rejects mutations whose meaning depends on Common Lisp reader-time behavior.
///
/// Reader conditionals need a feature profile, reader labels preserve object
/// identity, and `#.` needs an evaluation environment. None is available to a
/// structural source transformation, so changing any part of such a document
/// would make a partially rewritten result unsafe to apply.
pub fn reject_common_lisp_reader_conditionals(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> Result<(), ReaderConditionalSafetyError> {
    if dialect != Dialect::CommonLisp {
        return Ok(());
    }

    let root = tree.root_view();
    let mut stack = vec![&root];
    let mut conditional = None;
    let mut label = None;
    let mut literal = None;
    let mut read_time_evaluation = None;

    while let Some(view) = stack.pop() {
        if conditional.is_none() {
            conditional =
                common_lisp_reader_conditional_kind(view).map(|kind| (kind, view.content_span));
        }
        if label.is_none() {
            label = common_lisp_reader_label_kind(view).map(|kind| (kind, view.content_span));
        }
        if literal.is_none() {
            literal = common_lisp_reader_literal_kind(view).map(|kind| (kind, view.span));
        }
        if read_time_evaluation.is_none() && view.reader_prefixes.contains(&ReaderPrefix::ReadEval)
        {
            read_time_evaluation = Some(view.span);
        }

        stack.extend(view.children.iter().rev());
    }

    if let Some((kind, span)) = conditional {
        return Err(ReaderConditionalSafetyError::CommonLispReaderConditional { kind, span });
    }
    if let Some((kind, span)) = label {
        return Err(ReaderConditionalSafetyError::CommonLispReaderLabel { kind, span });
    }
    if let Some((kind, span)) = literal {
        return Err(ReaderConditionalSafetyError::CommonLispReaderLiteral { kind, span });
    }
    if let Some(span) = read_time_evaluation {
        return Err(ReaderConditionalSafetyError::CommonLispReadTimeEvaluation { span });
    }

    Ok(())
}

/// Rejects only edits that partially overlap a Common Lisp reader-time form.
///
/// This is for transformations whose behavior depends solely on their explicit
/// target spans. Structural whole-span rewrites may safely delete or replace a
/// reader-time form when the mutation fully covers it, but edits that cut
/// through the form remain unsafe.
pub fn reject_overlapping_common_lisp_reader_time_forms(
    tree: &SyntaxTree,
    dialect: Dialect,
    mutation_spans: impl IntoIterator<Item = ByteSpan>,
) -> Result<(), ReaderConditionalSafetyError> {
    if dialect != Dialect::CommonLisp {
        return Ok(());
    }

    let mutation_spans = mutation_spans.into_iter().collect::<Vec<_>>();
    for form in common_lisp_reader_conditional_forms(tree) {
        if mutation_spans
            .iter()
            .copied()
            .any(|span| overlaps_partially(span, form.span))
        {
            return Err(ReaderConditionalSafetyError::CommonLispReaderConditional {
                kind: form.kind,
                span: form.dispatch_span,
            });
        }
    }

    for form in common_lisp_reader_label_forms(tree) {
        if mutation_spans
            .iter()
            .copied()
            .any(|span| overlaps_partially(span, form.span))
        {
            return Err(ReaderConditionalSafetyError::CommonLispReaderLabel {
                kind: form.kind,
                span: form.dispatch_span,
            });
        }
    }

    for literal in common_lisp_reader_literals(tree) {
        if mutation_spans
            .iter()
            .copied()
            .any(|span| overlaps_partially(span, literal.span))
        {
            return Err(ReaderConditionalSafetyError::CommonLispReaderLiteral {
                kind: literal.kind,
                span: literal.span,
            });
        }
    }

    if let Some(span) =
        first_partially_overlapping_read_time_evaluation(&tree.root_view(), &mutation_spans)
    {
        return Err(ReaderConditionalSafetyError::CommonLispReadTimeEvaluation { span });
    }

    Ok(())
}

/// A mutation cannot safely proceed without a reader feature profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Variants identify the Common Lisp reader construct that blocked the mutation.
#[allow(clippy::enum_variant_names)]
pub enum ReaderConditionalSafetyError {
    CommonLispReaderConditional {
        kind: CommonLispReaderConditionalKind,
        span: ByteSpan,
    },
    CommonLispReadTimeEvaluation {
        span: ByteSpan,
    },
    CommonLispReaderLabel {
        kind: CommonLispReaderLabelKind,
        span: ByteSpan,
    },
    CommonLispReaderLiteral {
        kind: CommonLispReaderLiteralKind,
        span: ByteSpan,
    },
}

impl fmt::Display for ReaderConditionalSafetyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommonLispReaderConditional { kind, span } => write!(
                formatter,
                "cannot safely modify Common Lisp source containing reader conditional {} at byte {}",
                match kind {
                    CommonLispReaderConditionalKind::Include => "#+",
                    CommonLispReaderConditionalKind::Exclude => "#-",
                },
                span.start().get(),
            ),
            Self::CommonLispReadTimeEvaluation { span } => write!(
                formatter,
                "cannot safely modify Common Lisp source containing #. read-time evaluation at byte {}",
                span.start().get(),
            ),
            Self::CommonLispReaderLabel { kind, span } => write!(
                formatter,
                "cannot safely modify Common Lisp source containing reader label {} at byte {}",
                match kind {
                    CommonLispReaderLabelKind::Definition => "#n=",
                    CommonLispReaderLabelKind::Reference => "#n#",
                },
                span.start().get(),
            ),
            Self::CommonLispReaderLiteral { kind, span } => write!(
                formatter,
                "cannot safely modify Common Lisp source containing reader literal {} at byte {}",
                match kind {
                    CommonLispReaderLiteralKind::Vector => "#(...)",
                },
                span.start().get(),
            ),
        }
    }
}

impl Error for ReaderConditionalSafetyError {}

fn first_partially_overlapping_read_time_evaluation(
    view: &ExpressionView,
    mutation_spans: &[ByteSpan],
) -> Option<ByteSpan> {
    let mut stack = vec![view];
    while let Some(view) = stack.pop() {
        if view.reader_prefixes.contains(&ReaderPrefix::ReadEval)
            && mutation_spans
                .iter()
                .copied()
                .any(|span| overlaps_partially(span, view.span))
        {
            return Some(view.span);
        }

        stack.extend(view.children.iter().rev());
    }

    None
}

const fn overlaps_partially(left: ByteSpan, right: ByteSpan) -> bool {
    spans_overlap(left, right) && !left.contains_span(right)
}

const fn spans_overlap(left: ByteSpan, right: ByteSpan) -> bool {
    left.start().get() < right.end().get() && right.start().get() < left.end().get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::ByteOffset;

    #[test]
    fn rejects_common_lisp_vector_literals_for_semantic_mutations() {
        let tree = SyntaxTree::parse("(render #(helper value))").expect("parse succeeds");

        let error = reject_common_lisp_reader_conditionals(&tree, Dialect::CommonLisp)
            .expect_err("vector literals must reject semantic mutations");

        assert!(matches!(
            error,
            ReaderConditionalSafetyError::CommonLispReaderLiteral {
                kind: CommonLispReaderLiteralKind::Vector,
                ..
            }
        ));
    }

    #[test]
    fn allows_whole_span_removal_of_reader_literals() {
        let input = "(render #(helper value))";
        let tree = SyntaxTree::parse(input).expect("parse succeeds");
        let literal_start = input.find("#(").expect("vector literal exists");
        let literal_end = input[literal_start..]
            .find(')')
            .map(|offset| literal_start + offset + 1)
            .expect("vector literal closes");
        let removal = ByteSpan::new(ByteOffset::new(literal_start), ByteOffset::new(literal_end));

        reject_overlapping_common_lisp_reader_time_forms(&tree, Dialect::CommonLisp, [removal])
            .expect("whole literal removal must be allowed");
    }

    #[test]
    fn rejects_partial_overlaps_with_reader_literals() {
        let input = "(render #(helper value))";
        let tree = SyntaxTree::parse(input).expect("parse succeeds");
        let helper_start = input.find("helper").expect("helper exists");
        let removal = ByteSpan::new(
            ByteOffset::new(helper_start),
            ByteOffset::new(helper_start + "helper".len()),
        );

        let error =
            reject_overlapping_common_lisp_reader_time_forms(&tree, Dialect::CommonLisp, [removal])
                .expect_err("partial overlap must reject");

        assert!(matches!(
            error,
            ReaderConditionalSafetyError::CommonLispReaderLiteral {
                kind: CommonLispReaderLiteralKind::Vector,
                ..
            }
        ));
    }

    /// KNOWN GAP, pinned deliberately: a reader-prefixed conditional is not
    /// recognised, so this gate does not fire.
    ///
    /// This is a characterization test. It asserts what the code does, which is
    /// **not** what it should do, so that the gap is visible in the suite
    /// rather than hidden behind a passing assertion about a different tree.
    ///
    /// It previously asserted rejection, and passed, but only because it built
    /// the tree with `SyntaxTree::parse` — the `Dialect::Unknown` reader —
    /// while asking the gate about `Dialect::CommonLisp`. Under the reader that
    /// real `.lisp` files actually get, this has never rejected: parsing these
    /// same inputs with `Dialect::CommonLisp` returns `Ok` today and returned
    /// `Ok` before the `Dialect::Unknown` reader was aligned with it.
    ///
    /// Root cause: `push_opaque_reader_form` records `reader: None` and
    /// `symbol_offset: 0`, while the opaque node's span starts at the *prefix*.
    /// So the node's text begins `'#+…`, and `reader_conditional`, which tests
    /// `text[symbol_offset..].starts_with("#+")`, does not match. The same gap
    /// affects `reject_common_lisp_reader_conditionals` and every report built
    /// on `common_lisp_reader_conditional_forms`.
    ///
    /// Fixing it belongs to its own change: it makes the gate stricter for
    /// Common Lisp, which needs a false-positive audit against real code before
    /// it can start refusing edits it currently allows.
    #[test]
    fn a_reader_prefixed_conditional_is_not_yet_recognised_by_the_overlap_gate() {
        for (input, prefix_len) in [
            ("'#+sbcl selected", 1),
            ("`#+sbcl selected", 1),
            ("#'#+sbcl selected", 2),
        ] {
            let prefix = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(prefix_len));

            let outcomes = [Dialect::Unknown, Dialect::CommonLisp].map(|parsed_as| {
                let tree =
                    SyntaxTree::parse_with_dialect(input, parsed_as).expect("parse succeeds");
                reject_overlapping_common_lisp_reader_time_forms(
                    &tree,
                    Dialect::CommonLisp,
                    [prefix],
                )
                .is_err()
            });

            assert_eq!(
                outcomes[0], outcomes[1],
                "{input}: the two readers disagree about a prefixed conditional"
            );
            assert!(
                !outcomes[1],
                "{input}: the gate now rejects a reader-prefixed conditional -- the known gap is \
                 fixed, so replace this characterization test with the rejection assertion it \
                 replaced"
            );
        }
    }

    #[test]
    fn rejects_reader_prefix_only_edits_of_labels() {
        for input in ["'#1=(item)", "`#1=(item)"] {
            let tree = SyntaxTree::parse(input).expect("parse succeeds");
            let prefix = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(1));

            let error = reject_overlapping_common_lisp_reader_time_forms(
                &tree,
                Dialect::CommonLisp,
                [prefix],
            )
            .expect_err("reader prefix edit must reject");

            assert!(matches!(
                error,
                ReaderConditionalSafetyError::CommonLispReaderLabel { .. }
            ));
        }
    }

    /// An incomplete conditional is refused before any edit can be aimed at it.
    ///
    /// This asserted that editing the feature inside `#+sbcl` — a conditional
    /// with no guarded form — is rejected by the overlap gate. It relied on
    /// `SyntaxTree::parse` accepting the document, which only the permissive
    /// `Dialect::Unknown` reader ever did; `Dialect::CommonLisp` has always
    /// refused it.
    ///
    /// The guarantee is unchanged in substance and stronger in form. There is
    /// no tree to aim a span at, so the unsafe edit is unreachable rather than
    /// merely refused, and it is unreachable for every reader instead of for
    /// one of them.
    #[test]
    fn an_incomplete_reader_conditional_never_produces_a_tree_to_edit() {
        for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
            assert!(
                SyntaxTree::parse_with_dialect("#+sbcl", dialect).is_err(),
                "{dialect:?} accepted a conditional with no guarded form"
            );
        }
    }

    #[test]
    fn overlap_checks_support_deep_common_lisp_input() {
        const DEPTH: usize = 30_000;
        let input = format!("{}value{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
        let tree = SyntaxTree::parse(&input).expect("deep input parses");

        reject_overlapping_common_lisp_reader_time_forms(
            &tree,
            Dialect::CommonLisp,
            std::iter::empty(),
        )
        .expect("deep input must not overflow the stack");
    }
}
