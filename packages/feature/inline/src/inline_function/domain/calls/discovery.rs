use anyhow::Result;

use crate::domain::callable_scope::{
    common_lisp_local_callable_form, local_callable_binding_body_scope, local_callable_body_scope,
};
use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

use super::super::syntax::{list_head, spans_overlap};
use super::inline_function_symbol_reference_eq;

struct InlineCallTraversal<'a> {
    dialect: Dialect,
    definition_span: ByteSpan,
    function_name: &'a SymbolName,
}

pub(super) fn discover_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    definition_span: ByteSpan,
    function_name: &SymbolName,
) -> Result<Vec<Path>> {
    let context = InlineCallTraversal {
        dialect,
        definition_span,
        function_name,
    };
    let mut call_paths = Vec::new();
    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let selection = tree.select_path(&path)?;
        let view = selection.view();
        collect_function_call_paths(&view, path, &context, &[], &mut call_paths);
    }

    call_paths.sort_by_key(|path| {
        tree.select_path(path)
            .map(|selection| selection.span().start().get())
            .unwrap_or(usize::MAX)
    });
    Ok(call_paths)
}

fn collect_function_call_paths(
    view: &ExpressionView,
    path: Path,
    context: &InlineCallTraversal,
    local_callables: &[String],
    output: &mut Vec<Path>,
) {
    if let Some(head) = list_head(view) {
        if let Some(form) = common_lisp_local_callable_form(context.dialect, head) {
            collect_local_callable_function_call_paths(
                view,
                path,
                context,
                local_callables,
                form,
                output,
            );
            return;
        }
    }

    if view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && !spans_overlap(context.definition_span, view.span)
        && list_head(view).is_some_and(|head| {
            inline_function_symbol_reference_eq(
                context.dialect,
                head,
                context.function_name.as_str(),
            )
        })
        && !is_inline_function_local_callable_bound(
            context.dialect,
            local_callables,
            context.function_name.as_str(),
        )
    {
        output.push(path.clone());
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_function_call_paths(child, path.child(index), context, local_callables, output);
    }
}

fn is_inline_function_local_callable_bound(dialect: Dialect, scope: &[String], head: &str) -> bool {
    scope
        .iter()
        .any(|name| inline_function_symbol_reference_eq(dialect, name, head))
}

fn collect_local_callable_function_call_paths(
    view: &ExpressionView,
    path: Path,
    context: &InlineCallTraversal,
    local_callables: &[String],
    form: CommonLispLocalCallableForm,
    output: &mut Vec<Path>,
) {
    let body_scope = local_callable_body_scope(local_callables, view);

    if let Some(bindings) = view.children.get(1) {
        let binding_body_scope =
            local_callable_binding_body_scope(form, local_callables, &body_scope);
        for (binding_index, binding) in bindings.children.iter().enumerate() {
            for (child_index, child) in binding.children.iter().enumerate().skip(2) {
                collect_function_call_paths(
                    child,
                    path.descendant([1, binding_index, child_index]),
                    context,
                    binding_body_scope,
                    output,
                );
            }
        }
    }

    for (index, child) in view.children.iter().enumerate().skip(2) {
        collect_function_call_paths(child, path.child(index), context, &body_scope, output);
    }
}
