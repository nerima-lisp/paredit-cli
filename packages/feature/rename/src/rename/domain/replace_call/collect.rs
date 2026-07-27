use anyhow::{Context, Result};

use crate::domain::callable_scope::{
    common_lisp_local_callable_form, local_callable_binding_body_scope, local_callable_body_scope,
    local_callable_scope_at_path,
};
use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::definition::macro_expander_body_range;
use crate::domain::dialect::Dialect;
use crate::domain::rename::call_identity::is_local_call_bound;
use crate::domain::rename::reader::{
    apply_reader_prefix_context, executable_reader_context_at_path,
};
use crate::domain::rename::selection::list_head;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName, SyntaxTree};

use super::ReplaceFunctionCallSite;
use super::call_site::replace_call_site_from_view;

pub(super) fn collect_all_replace_call_sites(
    tree: &SyntaxTree,
    dialect: Dialect,
    input: &str,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<ReplaceFunctionCallSite>> {
    let mut calls = Vec::new();
    let ctx = ReplaceCallTraversal {
        dialect,
        input,
        from,
        to,
    };
    for (index, _) in tree.root_children().iter().enumerate() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_replace_call_sites_from_view(&view, path, &ctx, &[], 0, false, &mut calls)?;
    }
    calls.sort_by_key(|site| site.head_span.start());
    Ok(calls)
}

pub(super) fn collect_explicit_replace_call_sites(
    tree: &SyntaxTree,
    dialect: Dialect,
    input: &str,
    paths: &[Path],
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<ReplaceFunctionCallSite>> {
    let mut calls = Vec::new();
    for path in paths {
        let view = tree.select_path(path)?.view();
        if !executable_reader_context_at_path(tree, dialect, path)? {
            anyhow::bail!("call-path {path} is not in an executable reader context");
        }
        let local_callables = local_callable_scope_at_path(tree, dialect, path)?;
        if is_local_call_bound(dialect, &local_callables, from.as_str()) {
            anyhow::bail!("call-path {path} is shadowed by a local callable named {from}");
        }
        let site = replace_call_site_from_view(&view, dialect, input, path.to_string(), from, to)
            .with_context(|| format!("call-path {path} is not a call to {from}"))?;
        calls.push(site);
    }
    calls.sort_by_key(|site| site.head_span.start());
    Ok(calls)
}

struct ReplaceCallTraversal<'a> {
    dialect: Dialect,
    input: &'a str,
    from: &'a SymbolName,
    to: &'a SymbolName,
}

fn collect_replace_call_sites_from_view(
    view: &ExpressionView,
    path: Path,
    ctx: &ReplaceCallTraversal<'_>,
    local_callables: &[String],
    quasiquote_depth: usize,
    in_macro_expander: bool,
    calls: &mut Vec<ReplaceFunctionCallSite>,
) -> Result<()> {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return Ok(());
    };
    if quasiquote_depth > 0 && !in_macro_expander {
        for (index, child) in view.children.iter().enumerate() {
            collect_replace_call_sites_from_view(
                child,
                path.child(index),
                ctx,
                local_callables,
                quasiquote_depth,
                in_macro_expander,
                calls,
            )?;
        }
        return Ok(());
    }

    let head = list_head(view);
    if let Some(head) = head {
        if let Some(form) = common_lisp_local_callable_form(ctx.dialect, head) {
            collect_local_callable_replace_call_sites(
                view,
                path,
                ctx,
                local_callables,
                form,
                quasiquote_depth,
                in_macro_expander,
                calls,
            )?;
            return Ok(());
        }
    }

    let macro_expander_body =
        head.and_then(|head| macro_expander_body_range(ctx.dialect, view, head));

    if !is_local_call_bound(ctx.dialect, local_callables, ctx.from.as_str()) {
        if let Some(site) = replace_call_site_from_view(
            view,
            ctx.dialect,
            ctx.input,
            path.to_string(),
            ctx.from,
            ctx.to,
        ) {
            calls.push(site);
        }
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_replace_call_sites_from_view(
            child,
            path.child(index),
            ctx,
            local_callables,
            quasiquote_depth,
            in_macro_expander
                || macro_expander_body.is_some_and(|body_range| body_range.contains_child(index)),
            calls,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_local_callable_replace_call_sites(
    view: &ExpressionView,
    path: Path,
    ctx: &ReplaceCallTraversal<'_>,
    local_callables: &[String],
    form: CommonLispLocalCallableForm,
    quasiquote_depth: usize,
    in_macro_expander: bool,
    calls: &mut Vec<ReplaceFunctionCallSite>,
) -> Result<()> {
    let body_scope = local_callable_body_scope(local_callables, view);

    if let Some(bindings) = view.children.get(1) {
        let binding_body_scope =
            local_callable_binding_body_scope(form, local_callables, &body_scope);
        for (binding_index, binding) in bindings.children.iter().enumerate() {
            for (child_index, child) in binding.children.iter().enumerate().skip(2) {
                collect_replace_call_sites_from_view(
                    child,
                    path.descendant([1, binding_index, child_index]),
                    ctx,
                    binding_body_scope,
                    quasiquote_depth,
                    in_macro_expander || form.is_macro(),
                    calls,
                )?;
            }
        }
    }

    for (index, child) in view.children.iter().enumerate().skip(2) {
        collect_replace_call_sites_from_view(
            child,
            path.child(index),
            ctx,
            &body_scope,
            quasiquote_depth,
            in_macro_expander,
            calls,
        )?;
    }

    Ok(())
}
