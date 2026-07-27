use anyhow::{Context, Result};

use paredit_core_syntax::common_lisp::{CommonLispOperator, common_lisp_symbol_reference_eq};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

use super::super::list_edit::{atom_child, atom_text};
use super::insertion::{
    keyword_parameter_insertion, optional_parameter_insertion, positional_parameter_insertion,
};
use super::lambda_list::parameter_locations;
use super::types::{FunctionParameterDefinitionScope, FunctionParameterTarget};

pub fn parse_remove_function_parameter_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
) -> Result<FunctionParameterTarget> {
    parse_function_parameter_definition(dialect, tree, path, None, "remove-function-parameter")
}

pub fn parse_move_function_parameter_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
) -> Result<FunctionParameterTarget> {
    parse_function_parameter_definition(dialect, tree, path, None, "move-function-parameter")
}

pub fn parse_swap_function_parameters_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
) -> Result<FunctionParameterTarget> {
    parse_function_parameter_definition(dialect, tree, path, None, "swap-function-parameters")
}

pub fn parse_reorder_function_parameters_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
) -> Result<FunctionParameterTarget> {
    parse_function_parameter_definition(dialect, tree, path, None, "reorder-function-parameters")
}

pub fn parse_add_function_parameter_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
    new_parameter: &SymbolName,
) -> Result<FunctionParameterTarget> {
    parse_function_parameter_definition(
        dialect,
        tree,
        path,
        Some(new_parameter),
        "add-function-parameter",
    )
}

fn parse_function_parameter_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
    new_parameter: Option<&SymbolName>,
    operation: &str,
) -> Result<FunctionParameterTarget> {
    let view = tree.select_path(path)?.view();
    if let Some(target) =
        parse_local_callable_binding_definition(dialect, tree, path, &view, new_parameter)?
    {
        return Ok(target);
    }

    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("{operation} definition selection must be a function definition list");
    }
    if view.children.len() < 3 {
        anyhow::bail!("{operation} definition must include a name and parameters");
    }

    let head = atom_text(&view.children[0])
        .with_context(|| format!("{operation} definition must start with a definition atom"))?;
    let shape = definition_shape(dialect, &view, head);
    let recognized_common_lisp_head = matches!(dialect, Dialect::CommonLisp | Dialect::Unknown)
        && CommonLispOperator::from_head(head).is_some();
    let supports_head = dialect.supports_function_parameter_refactor_head(head);
    let supports_generic_callable_shape = !recognized_common_lisp_head
        && shape.is_some_and(|shape| {
            shape.category.is_callable() && shape.lambda_list(&view).is_some()
        });
    if !supports_head && !supports_generic_callable_shape {
        anyhow::bail!(
            "{}",
            unsupported_function_parameter_definition_message(head, operation)
        );
    }

    let (
        function_name,
        parameter_container,
        call_argument_offset,
        protected_prefix_count,
        allow_specialized_required_parameters,
    ) = if head == "define" {
        let signature = view.children.get(1).context(
            "scheme define selection must include a signature list: (define (name args...) body)",
        )?;
        if signature.kind != ExpressionKind::List || signature.delimiter != Some(Delimiter::Paren) {
            anyhow::bail!(
                "{operation} currently supports scheme procedure defines, not variable defines"
            );
        }
        let name = atom_child(signature, 0)
            .context("scheme define signature must start with a function name")?;
        (
            SymbolName::new(name.to_owned())?,
            signature.clone(),
            0,
            1,
            false,
        )
    } else if shape.is_some_and(|shape| shape.category == DefinitionCategory::Method) {
        let name =
            atom_child(&view, 1).context("function definition must include a symbol name")?;
        let params = shape
            .and_then(|shape| shape.lambda_list(&view))
            .with_context(|| {
                format!("{operation} defmethod definition must include a specialized lambda list")
            })?;
        (
            SymbolName::new(name.to_owned())?,
            params.clone(),
            0,
            0,
            true,
        )
    } else {
        let name =
            atom_child(&view, 1).context("function definition must include a symbol name")?;
        let params = view
            .children
            .get(2)
            .context("function definition must include a parameter list")?;
        if matches!(dialect, Dialect::CommonLisp | Dialect::Unknown)
            && CommonLispOperator::from_head(head) == Some(CommonLispOperator::Defsetf)
            && (params.kind != ExpressionKind::List || params.delimiter != Some(Delimiter::Paren))
        {
            anyhow::bail!(
                "{operation} does not support short-form defsetf; select a long-form defsetf with an accessor lambda list"
            );
        }
        (
            SymbolName::new(name.to_owned())?,
            params.clone(),
            if matches!(dialect, Dialect::CommonLisp | Dialect::Unknown)
                && CommonLispOperator::from_head(head)
                    == Some(CommonLispOperator::DefineModifyMacro)
            {
                1
            } else {
                0
            },
            0,
            false,
        )
    };
    let parameters = parameter_locations(
        dialect,
        &parameter_container,
        protected_prefix_count,
        allow_specialized_required_parameters,
        operation,
    )?;
    let has_lambda_list_marker = parameter_container.children[protected_prefix_count..]
        .iter()
        .any(|child| atom_text(child).is_some_and(|name| name.starts_with('&')));
    let keyword_parameter_insertion = match new_parameter {
        Some(new_parameter) => keyword_parameter_insertion(
            dialect,
            &parameter_container,
            protected_prefix_count,
            new_parameter,
        )?,
        None => None,
    };
    let positional_parameter_insertion = match new_parameter {
        Some(_) => {
            positional_parameter_insertion(dialect, &parameter_container, protected_prefix_count)?
        }
        None => None,
    };
    let optional_parameter_insertion = match new_parameter {
        Some(_) => {
            optional_parameter_insertion(dialect, &parameter_container, protected_prefix_count)?
        }
        None => None,
    };

    if let Some(new_parameter) = new_parameter {
        if new_parameter.as_str().starts_with(['&', ':']) {
            anyhow::bail!(
                "add-function-parameter found invalid parameter symbol '{new_parameter}'"
            );
        }
        if parameters.iter().any(|parameter| {
            common_lisp_symbol_reference_eq(&parameter.name, new_parameter.as_str())
        }) {
            anyhow::bail!(
                "add-function-parameter parameter '{new_parameter}' already exists in {function_name}"
            );
        }
    }

    Ok(FunctionParameterTarget {
        function_name,
        parameter_container,
        call_argument_offset,
        protected_prefix_count,
        definition_span: view.span,
        definition_scope: FunctionParameterDefinitionScope::TopLevel,
        has_lambda_list_marker,
        positional_parameter_insertion,
        keyword_parameter_insertion,
        optional_parameter_insertion,
        parameters,
    })
}

fn parse_local_callable_binding_definition(
    dialect: Dialect,
    tree: &SyntaxTree,
    path: &Path,
    view: &ExpressionView,
    new_parameter: Option<&SymbolName>,
) -> Result<Option<FunctionParameterTarget>> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return Ok(None);
    }
    if view.children.len() < 2 {
        return Ok(None);
    }

    let Some(binding_list_path) = path.parent() else {
        return Ok(None);
    };
    let Some(form_path) = binding_list_path.parent() else {
        return Ok(None);
    };

    let binding_list_view = tree.select_path(&binding_list_path)?.view();
    if binding_list_view.kind != ExpressionKind::List
        || binding_list_view.delimiter != Some(Delimiter::Paren)
    {
        return Ok(None);
    }

    let form_view = tree.select_path(&form_path)?.view();
    if form_view.kind != ExpressionKind::List || form_view.delimiter != Some(Delimiter::Paren) {
        return Ok(None);
    }
    if form_view.children.len() < 2 {
        return Ok(None);
    }

    let head = match atom_text(&form_view.children[0]) {
        Some(head) => head,
        None => return Ok(None),
    };
    let Some(form) = dialect.common_lisp_local_callable_form_for_head(head) else {
        return Ok(None);
    };

    let expected_binding_path = form_path.child(1);
    if binding_list_path != expected_binding_path {
        return Ok(None);
    }

    let function_name =
        atom_child(view, 0).context("local callable binding must include a symbol name")?;
    let parameter_container = view
        .children
        .get(1)
        .context("local callable binding must include a lambda list")?
        .clone();
    if parameter_container.kind != ExpressionKind::List
        || parameter_container.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("{head} binding must include a parenthesized lambda list");
    }

    let function_name = SymbolName::new(function_name.to_owned())?;
    let parameters = parameter_locations(
        dialect,
        &parameter_container,
        0,
        false,
        operation_name_for_local_callable_form(head),
    )?;
    let has_lambda_list_marker = parameter_container
        .children
        .iter()
        .any(|child| atom_text(child).is_some_and(|name| name.starts_with('&')));

    Ok(Some(FunctionParameterTarget {
        function_name,
        parameter_container: parameter_container.clone(),
        call_argument_offset: 0,
        protected_prefix_count: 0,
        definition_span: view.span,
        definition_scope: FunctionParameterDefinitionScope::LocalCallableBinding {
            form,
            enclosing_form_span: form_view.span,
        },
        has_lambda_list_marker,
        positional_parameter_insertion: positional_parameter_insertion(
            dialect,
            &parameter_container,
            0,
        )?,
        keyword_parameter_insertion: match new_parameter {
            Some(new_parameter) => {
                keyword_parameter_insertion(dialect, &parameter_container, 0, new_parameter)?
            }
            None => None,
        },
        optional_parameter_insertion: match new_parameter {
            Some(_) => optional_parameter_insertion(dialect, &parameter_container, 0)?,
            None => None,
        },
        parameters,
    }))
}

fn operation_name_for_local_callable_form(head: &str) -> &str {
    match head {
        "flet" => "flet binding",
        "labels" => "labels binding",
        "macrolet" => "macrolet binding",
        "compiler-macrolet" => "compiler-macrolet binding",
        _ => "local callable binding",
    }
}

fn unsupported_function_parameter_definition_message(head: &str, operation: &str) -> String {
    format!("{operation} does not support definition head: {head}")
}
