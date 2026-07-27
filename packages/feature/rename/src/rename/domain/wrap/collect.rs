use anyhow::{Context, Result};

use crate::domain::callable_scope::{
    common_lisp_local_callable_form, local_callable_binding_body_scope, local_callable_body_scope,
    local_callable_scope_at_path,
};
use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::definition::{definition_shape, macro_expander_body_range};
use crate::domain::dialect::Dialect;
use crate::domain::rename::call_identity::{call_reference_eq, is_local_call_bound};
use crate::domain::rename::reader::{
    apply_reader_prefix_context, executable_reader_context_at_path,
};
use crate::domain::rename::selection::list_head;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName, SyntaxTree};

use super::call_site::wrap_call_site_from_view;
use super::choose::select_outermost_wrap_call_sites;
use super::{WrapFunctionCallSite, WrapFunctionCallTemplate};

pub(super) fn collect_wrap_all_call_sites(
    tree: &SyntaxTree,
    dialect: Dialect,
    input: &str,
    function: &SymbolName,
    wrapper: &SymbolName,
    template: Option<&WrapFunctionCallTemplate>,
) -> Result<(
    Vec<WrapFunctionCallSite>,
    Vec<WrapFunctionCallSite>,
    Vec<WrapFunctionCallSite>,
)> {
    let mut candidates = Vec::new();
    let mut skipped_already_wrapped = Vec::new();

    for (index, _) in tree.root_children().iter().enumerate() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let mut collection = WrapCallSiteCollection {
            dialect,
            input,
            function,
            wrapper,
            template,
            candidates: &mut candidates,
            skipped_already_wrapped: &mut skipped_already_wrapped,
        };
        collect_wrap_call_sites_from_view(&view, path, None, &[], 0, false, &mut collection);
    }

    let (calls, skipped_nested) = select_outermost_wrap_call_sites(candidates);
    Ok((calls, skipped_already_wrapped, skipped_nested))
}

pub(super) fn collect_wrap_explicit_call_sites(
    tree: &SyntaxTree,
    dialect: Dialect,
    input: &str,
    paths: &[Path],
    function: &SymbolName,
    wrapper: &SymbolName,
    template: Option<&WrapFunctionCallTemplate>,
) -> Result<(
    Vec<WrapFunctionCallSite>,
    Vec<WrapFunctionCallSite>,
    Vec<WrapFunctionCallSite>,
)> {
    let mut calls = Vec::new();
    let mut skipped_already_wrapped = Vec::new();

    for path in paths {
        let view = tree.select_path(path)?.view();
        if !executable_reader_context_at_path(tree, dialect, path)? {
            anyhow::bail!("call-path {path} is not in an executable reader context");
        }
        let local_callables = local_callable_scope_at_path(tree, dialect, path)?;
        if is_local_call_bound(dialect, &local_callables, function.as_str()) {
            anyhow::bail!("call-path {path} is shadowed by a local callable named {function}");
        }
        let site = wrap_call_site_from_view(
            &view,
            dialect,
            input,
            path.to_string(),
            function,
            wrapper,
            template,
        )
        .with_context(|| format!("call-path {path} is not a call to {function}"))?;
        if call_site_is_already_wrapped(tree, dialect, path, wrapper)? {
            skipped_already_wrapped.push(site);
        } else {
            calls.push(site);
        }
    }

    Ok((calls, skipped_already_wrapped, Vec::new()))
}

struct WrapCallSiteCollection<'a> {
    dialect: Dialect,
    input: &'a str,
    function: &'a SymbolName,
    wrapper: &'a SymbolName,
    template: Option<&'a WrapFunctionCallTemplate>,
    candidates: &'a mut Vec<WrapFunctionCallSite>,
    skipped_already_wrapped: &'a mut Vec<WrapFunctionCallSite>,
}

fn collect_wrap_call_sites_from_view(
    view: &ExpressionView,
    path: Path,
    parent_head: Option<&str>,
    local_callables: &[String],
    quasiquote_depth: usize,
    in_macro_expander: bool,
    collection: &mut WrapCallSiteCollection<'_>,
) {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return;
    };
    if quasiquote_depth > 0 && !in_macro_expander {
        for (index, child) in view.children.iter().enumerate() {
            collect_wrap_call_sites_from_view(
                child,
                path.child(index),
                None,
                local_callables,
                quasiquote_depth,
                in_macro_expander,
                collection,
            );
        }
        return;
    }

    if let Some(head) = list_head(view) {
        if let Some(form) = common_lisp_local_callable_form(collection.dialect, head) {
            collect_local_callable_wrap_call_sites(
                view,
                path,
                local_callables,
                form,
                quasiquote_depth,
                in_macro_expander,
                collection,
            );
            return;
        }
    }

    let current_head = list_head(view);
    if !is_local_call_bound(
        collection.dialect,
        local_callables,
        collection.function.as_str(),
    ) {
        if let Some(site) = wrap_call_site_from_view(
            view,
            collection.dialect,
            collection.input,
            path.to_string(),
            collection.function,
            collection.wrapper,
            collection.template,
        ) {
            if parent_head.is_some_and(|head| {
                call_reference_eq(collection.dialect, head, collection.wrapper.as_str())
            }) {
                collection.skipped_already_wrapped.push(site);
            } else if current_head
                .and_then(|head| definition_shape(collection.dialect, view, head))
                .is_none()
            {
                collection.candidates.push(site);
            }
        }
    }

    let macro_expander_body =
        current_head.and_then(|head| macro_expander_body_range(collection.dialect, view, head));

    for (index, child) in view.children.iter().enumerate() {
        collect_wrap_call_sites_from_view(
            child,
            path.child(index),
            current_head,
            local_callables,
            quasiquote_depth,
            in_macro_expander
                || macro_expander_body.is_some_and(|body_range| body_range.contains_child(index)),
            collection,
        );
    }
}

fn collect_local_callable_wrap_call_sites(
    view: &ExpressionView,
    path: Path,
    local_callables: &[String],
    form: CommonLispLocalCallableForm,
    quasiquote_depth: usize,
    in_macro_expander: bool,
    collection: &mut WrapCallSiteCollection<'_>,
) {
    let body_scope = local_callable_body_scope(local_callables, view);

    if let Some(bindings) = view.children.get(1) {
        let binding_body_scope =
            local_callable_binding_body_scope(form, local_callables, &body_scope);
        for (binding_index, binding) in bindings.children.iter().enumerate() {
            let binding_head = list_head(binding);
            for (child_index, child) in binding.children.iter().enumerate().skip(2) {
                collect_wrap_call_sites_from_view(
                    child,
                    path.descendant([1, binding_index, child_index]),
                    binding_head,
                    binding_body_scope,
                    quasiquote_depth,
                    in_macro_expander || form.is_macro(),
                    collection,
                );
            }
        }
    }

    let current_head = list_head(view);
    for (index, child) in view.children.iter().enumerate().skip(2) {
        collect_wrap_call_sites_from_view(
            child,
            path.child(index),
            current_head,
            &body_scope,
            quasiquote_depth,
            in_macro_expander,
            collection,
        );
    }
}

fn call_site_is_already_wrapped(
    tree: &SyntaxTree,
    dialect: Dialect,
    path: &Path,
    wrapper: &SymbolName,
) -> Result<bool> {
    let Some(parent_path) = path.parent() else {
        return Ok(false);
    };
    if parent_path.indexes().is_empty() {
        return Ok(false);
    }
    let parent = tree.select_path(&parent_path)?.view();
    let Some(head) = list_head(&parent) else {
        return Ok(false);
    };
    Ok(call_reference_eq(dialect, head, wrapper.as_str())
        && definition_shape(dialect, &parent, head).is_none())
}
