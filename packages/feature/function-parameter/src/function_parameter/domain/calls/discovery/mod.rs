mod local_callable;
mod shared;
mod top_level;

use anyhow::Result;

use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::dialect::Dialect;
use crate::domain::function_parameter::definition::FunctionParameterDefinitionScope;
use crate::domain::sexpr::{ByteSpan, Path, SymbolName, SyntaxTree};

use local_callable::discover_local_callable_binding_call_paths;
use top_level::discover_function_call_paths;

pub(in crate::domain::function_parameter) struct FunctionCallPathRequest<'a> {
    pub(in crate::domain::function_parameter) tree: &'a SyntaxTree,
    pub(in crate::domain::function_parameter) dialect: Dialect,
    pub(in crate::domain::function_parameter) explicit_call_paths: Vec<Path>,
    pub(in crate::domain::function_parameter) all_calls: bool,
    pub(in crate::domain::function_parameter) definition_span: ByteSpan,
    pub(in crate::domain::function_parameter) definition_scope: FunctionParameterDefinitionScope,
    pub(in crate::domain::function_parameter) function_name: &'a SymbolName,
    pub(in crate::domain::function_parameter) command: &'a str,
}

pub(in crate::domain::function_parameter) fn resolve_function_call_paths(
    request: FunctionCallPathRequest<'_>,
) -> Result<Vec<Path>> {
    if request.all_calls && !request.explicit_call_paths.is_empty() {
        anyhow::bail!(
            "{} accepts either --all-calls or repeated --call-path, not both",
            request.command
        );
    }

    if request.all_calls {
        let call_paths = discover_function_call_paths(
            request.tree,
            request.dialect,
            request.definition_span,
            request.function_name,
        )?;
        if call_paths.is_empty() {
            anyhow::bail!(
                "{} --all-calls found no same-file calls for {}",
                request.command,
                request.function_name
            );
        }
        return Ok(call_paths);
    }

    if request.explicit_call_paths.is_empty() {
        anyhow::bail!(
            "{} requires at least one --call-path or --all-calls",
            request.command
        );
    }

    validate_explicit_function_call_paths(
        request.tree,
        request.dialect,
        &request.explicit_call_paths,
        request.definition_span,
        request.definition_scope,
        request.function_name,
        request.command,
    )?;

    Ok(request.explicit_call_paths)
}

fn validate_explicit_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    explicit_call_paths: &[Path],
    definition_span: ByteSpan,
    definition_scope: FunctionParameterDefinitionScope,
    function_name: &SymbolName,
    command: &str,
) -> Result<()> {
    let discoverable_call_paths = match definition_scope {
        FunctionParameterDefinitionScope::TopLevel => {
            discover_function_call_paths(tree, dialect, definition_span, function_name)?
        }
        FunctionParameterDefinitionScope::LocalCallableBinding {
            form,
            enclosing_form_span,
        } => discover_local_callable_binding_call_paths(
            tree,
            dialect,
            definition_span,
            enclosing_form_span,
            function_name,
            form,
        )?,
    };
    for call_path in explicit_call_paths {
        let selection = tree.select_path(call_path)?;
        let view = selection.view();
        if view.kind != crate::domain::sexpr::ExpressionKind::List
            || view.delimiter != Some(crate::domain::sexpr::Delimiter::Paren)
        {
            anyhow::bail!("{command} --call-path {call_path} must select a function call list");
        }

        if !super::matches_function_call_view(&view, function_name) {
            let Some(head) = crate::domain::function_parameter::list_edit::list_head(&view) else {
                anyhow::bail!("{command} --call-path {call_path} must select a function call list");
            };
            anyhow::bail!(
                "{command} --call-path {call_path} head '{head}' does not match selected definition '{function_name}'"
            );
        }

        if !discoverable_call_paths.iter().any(|path| path == call_path) {
            anyhow::bail!(
                "{command} --call-path {call_path} resolves to a call shadowed by a local callable binding or overlaps the selected definition"
            );
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct FunctionCallTraversal<'a> {
    pub(super) dialect: Dialect,
    pub(super) definition_span: ByteSpan,
    pub(super) function_name: &'a SymbolName,
}

#[derive(Clone, Copy)]
pub(super) struct SelectedLocalCallableTraversal<'a> {
    pub(super) dialect: Dialect,
    pub(super) definition_span: ByteSpan,
    pub(super) enclosing_form_span: ByteSpan,
    pub(super) function_name: &'a SymbolName,
    pub(super) form: CommonLispLocalCallableForm,
}
