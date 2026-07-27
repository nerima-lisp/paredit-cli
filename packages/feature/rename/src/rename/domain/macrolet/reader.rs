use crate::domain::common_lisp::{common_lisp_operator_head_eq, common_lisp_symbol_reference_eq};
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SymbolName};

use super::RenameFunctionOccurrence;
use super::scope::{
    LocalCallableRenameKind, MacroletRenameScope, allows_function_reference_rename,
};
use super::traversal::{TraversalPath, TraversalPathArena};
pub(super) use crate::domain::rename::reader::{atom_symbol_span, atom_symbol_text};
use crate::domain::rename::selection::atom_text;

pub(super) struct CallableTarget<'a> {
    pub(super) span: ByteSpan,
    pub(super) text: &'a str,
    suffix: Option<usize>,
}

pub(super) fn callable_target(view: &ExpressionView) -> Option<CallableTarget<'_>> {
    if let Some(text) = atom_symbol_text(view) {
        return Some(CallableTarget {
            span: atom_symbol_span(view).unwrap_or(view.span),
            text,
            suffix: None,
        });
    }

    (view.kind == ExpressionKind::List).then_some(())?;
    let head = view.children.first().and_then(atom_text)?;
    common_lisp_operator_head_eq(head, "setf").then_some(())?;
    let name = view
        .children
        .get(1)
        .filter(|name| name.kind == ExpressionKind::Atom)?;
    Some(CallableTarget {
        span: atom_symbol_span(name).unwrap_or(name.span),
        text: atom_symbol_text(name)?,
        suffix: Some(1),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "reader designator handling needs rename kind, scope, and accumulator state"
)]
pub(super) fn collect_local_function_designator_renames(
    view: &ExpressionView,
    path: TraversalPath,
    paths: &mut TraversalPathArena,
    from: &SymbolName,
    to: &SymbolName,
    kind: LocalCallableRenameKind,
    scope: MacroletRenameScope,
    quasiquote_depth: usize,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    if kind != LocalCallableRenameKind::Function || quasiquote_depth > 0 {
        return false;
    }

    if view.reader_prefixes.contains(&ReaderPrefix::Function) {
        if let Some(target) = callable_target(view) {
            if !allows_function_reference_rename(scope, target.text) {
                return false;
            }
            return push_callable_target_rename_if_match(
                target,
                path,
                &[],
                paths,
                from,
                to,
                renames,
            );
        }
    }

    false
}

pub(super) fn push_callable_target_rename_if_match(
    target: CallableTarget<'_>,
    mut path: TraversalPath,
    prefix: &[usize],
    paths: &mut TraversalPathArena,
    from: &SymbolName,
    to: &SymbolName,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    if !common_lisp_symbol_reference_eq(target.text, from.as_str()) {
        return false;
    }

    for &index in prefix {
        path = paths.child(path, index);
    }
    if let Some(index) = target.suffix {
        path = paths.child(path, index);
    }
    renames.push(RenameFunctionOccurrence {
        path: paths.materialize(path).to_string(),
        span: target.span,
        text: target.text.to_owned(),
        replacement: to.as_str().to_owned(),
    });
    true
}

pub(super) fn push_atom_rename_if_match(
    view: &ExpressionView,
    path: TraversalPath,
    paths: &mut TraversalPathArena,
    from: &SymbolName,
    to: &SymbolName,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    let Some(text) = atom_symbol_text(view) else {
        return false;
    };
    if !common_lisp_symbol_reference_eq(text, from.as_str()) {
        return false;
    }

    renames.push(RenameFunctionOccurrence {
        path: paths.materialize(path).to_string(),
        span: atom_symbol_span(view).unwrap_or(view.span),
        text: text.to_owned(),
        replacement: to.as_str().to_owned(),
    });
    true
}
