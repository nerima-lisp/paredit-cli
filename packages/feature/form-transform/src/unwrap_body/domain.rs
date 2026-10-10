use crate::error::{FormTransformResult, TransformDialectError, TransformTargetError};
use paredit_core_edit::mutation_safety::reject_overlapping_common_lisp_reader_time_forms;
use paredit_core_edit::{ConservativeRefusal, DocumentRefusal};
use paredit_core_syntax::common_lisp::{
    common_lisp_symbol_name_eq, common_lisp_symbol_reference_eq,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

const OPERATION: &str = "unwrap-body";

#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    pub expected_function: Option<SymbolName>,
    pub body_start: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub dialect: Dialect,
    pub path: Path,
    pub function: SymbolName,
    pub span: ByteSpan,
    pub body_start: usize,
    pub body_span: ByteSpan,
    pub body_count: usize,
    pub call_argument_count: usize,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan(request: Request<'_>) -> FormTransformResult<Plan> {
    if request.dialect == Dialect::Unknown {
        return Err(TransformDialectError::RequiresKnown {
            operation: OPERATION,
        }
        .into());
    }
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputInvalid {
                operation: OPERATION,
                source,
            }
        })?;
    let mut expected = tree.root_view();
    let indexes = request.path.to_raw_indexes();
    let mut target = &expected;
    for &index in &indexes {
        reject_quoted(target, request.dialect)?;
        target = target.children.get(index).ok_or_else(|| {
            crate::error::TransformSelectorError::InvalidPath {
                path: request.path.to_string(),
            }
        })?;
    }
    reject_quoted(target, request.dialect)?;
    if target.kind != ExpressionKind::List || target.delimiter != Some(Delimiter::Paren) {
        return Err(TransformTargetError::UnwrapBodyNotAWrapper.into());
    }
    let head = target
        .children
        .first()
        .filter(|head| head.kind == ExpressionKind::Atom && head.reader_prefixes.is_empty())
        .and_then(|head| head.text.as_deref())
        .ok_or(TransformTargetError::UnwrapBodyNotAWrapper)?;
    let function = SymbolName::new(head)?;
    if let Some(guard) = &request.expected_function {
        let matches = if request.dialect == Dialect::CommonLisp {
            common_lisp_symbol_reference_eq(head, guard.as_str())
        } else {
            head == guard.as_str()
        };
        if !matches {
            return Err(TransformTargetError::UnwrapBodyFunctionMismatch {
                expected: guard.to_string(),
                found: function.to_string(),
            }
            .into());
        }
    }
    let argument_count = target.children.len().saturating_sub(1);
    let body_index = request
        .body_start
        .checked_add(1)
        .filter(|&index| index < target.children.len())
        .ok_or(TransformTargetError::UnwrapBodyMissing {
            index: request.body_start,
            count: argument_count,
        })?;
    let body = target.children[body_index..].to_vec();
    let (&target_index, parent_indexes) = indexes
        .split_last()
        .ok_or(TransformTargetError::UnwrapBodyNotAWrapper)?;
    let mut parent = &expected;
    for &index in parent_indexes {
        parent = &parent.children[index];
    }
    if body.len() > 1
        && parent.kind != ExpressionKind::Root
        && !(target_index > 0 && is_sequence(parent, request.dialect))
    {
        return Err(TransformTargetError::UnwrapBodyNotASequence.into());
    }
    reject_overlapping_common_lisp_reader_time_forms(&tree, request.dialect, [target.span])?;
    let raw_start = target.children[body_index - 1].span.end().get();
    let raw_end = target.span.end().get() - 1;
    if tree.has_comment_in(ByteSpan::new(
        target.span.start(),
        ByteOffset::new(raw_start),
    )) {
        return Err(ConservativeRefusal::Comments {
            operation: OPERATION,
        }
        .into());
    }
    let raw_body = &request.input[raw_start..raw_end];
    let replacement = raw_body.trim_start_matches(char::is_whitespace).to_owned();
    let body_span = ByteSpan::new(
        ByteOffset::new(raw_end - replacement.len()),
        ByteOffset::new(raw_end),
    );
    let span = target.span;
    let mut rewritten = request.input.to_owned();
    rewritten.replace_range(span.as_range(), &replacement);
    let rewritten_tree =
        SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
            DocumentRefusal::RewrittenNotParseable {
                operation: OPERATION,
                source,
            }
        })?;
    let mut parent = &mut expected;
    for &index in parent_indexes {
        parent = &mut parent.children[index];
    }
    parent
        .children
        .splice(target_index..=target_index, body.iter().cloned());
    // Parsing alone accepts fused atoms and line comments that swallow following forms.
    if !same_syntax(&expected, &rewritten_tree.root_view()) {
        return Err(TransformTargetError::UnwrapBodyBoundaryChanged.into());
    }
    Ok(Plan {
        dialect: request.dialect,
        path: request.path,
        function,
        span,
        body_start: request.body_start,
        body_span,
        body_count: body.len(),
        call_argument_count: argument_count,
        changed: span.slice(request.input) != replacement,
        replacement,
        rewritten,
    })
}

fn reject_quoted(view: &ExpressionView, dialect: Dialect) -> FormTransformResult<()> {
    let quoted_head = view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && view
            .children
            .first()
            .and_then(|head| head.text.as_deref())
            .is_some_and(|head| {
                ["quote", "quasiquote", "function"].iter().any(|quoted| {
                    if dialect == Dialect::CommonLisp {
                        common_lisp_symbol_reference_eq(head, quoted)
                    } else {
                        head == *quoted
                    }
                })
            });
    if !view.reader_prefixes.is_empty() || quoted_head {
        return Err(ConservativeRefusal::ReaderPrefixedOrQuoted {
            operation: OPERATION,
        }
        .into());
    }
    Ok(())
}

fn is_sequence(view: &ExpressionView, dialect: Dialect) -> bool {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return false;
    }
    let Some(head) = view
        .children
        .first()
        .filter(|head| head.reader_prefixes.is_empty())
        .and_then(|head| head.text.as_deref())
    else {
        return false;
    };
    match dialect {
        Dialect::CommonLisp => common_lisp_symbol_name_eq(head, "progn"),
        Dialect::EmacsLisp => head == "progn",
        Dialect::Scheme | Dialect::Racket => head == "begin",
        Dialect::Clojure => head == "do",
        _ => false,
    }
}

fn same_syntax(left: &ExpressionView, right: &ExpressionView) -> bool {
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        if left.kind != right.kind
            || left.delimiter != right.delimiter
            || left.reader_prefixes != right.reader_prefixes
            || left.text != right.text
            || left.children.len() != right.children.len()
        {
            return false;
        }
        pending.extend(left.children.iter().zip(&right.children));
    }
    true
}

#[cfg(test)]
mod tests;
