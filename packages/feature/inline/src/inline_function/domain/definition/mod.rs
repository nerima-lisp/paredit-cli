use anyhow::{Context, Result};

use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

mod body;
mod destructure;
mod lambda_list;
mod types;

use super::syntax::{atom_child, atom_text};
use body::{
    effective_body_forms, inline_function_body_view, unsupported_inline_function_definition_message,
};
use lambda_list::{inline_parameter_names, inline_parameter_names_from_children};

pub(super) use types::{
    InlineDefinition, InlineDefinitionKind, InlineDestructureKeyPattern,
    InlineDestructureListPattern, InlineDestructureOptionalPattern, InlineDestructurePattern,
    InlineParameter, InlineParameterBinding, InlineParameterKind,
};

pub(super) fn parse_inline_function_definition(
    dialect: Dialect,
    input: &str,
    view: ExpressionView,
) -> Result<InlineDefinition> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("inline-function definition selection must be a function definition list");
    }
    if view.children.len() < 3 {
        anyhow::bail!(
            "inline-function definition must include a name, parameters, and a body expression"
        );
    }

    let head = atom_text(&view.children[0])
        .context("inline-function definition must start with a definition atom")?;
    let definition_kind = match definition_shape(dialect, &view, head).map(|shape| shape.category) {
        Some(DefinitionCategory::Macro) => {
            if !dialect.supports_inline_function_refactor_head(head) {
                anyhow::bail!(
                    "{}",
                    unsupported_inline_function_definition_message(dialect, head)
                );
            }
            InlineDefinitionKind::Macro
        }
        _ if !dialect.supports_inline_function_refactor_head(head) => {
            anyhow::bail!(
                "{}",
                unsupported_inline_function_definition_message(dialect, head)
            );
        }
        _ => InlineDefinitionKind::Function,
    };

    let (name, params, accepts_other_keys, body_start) = if head == "define" {
        let signature = view.children.get(1).context(
            "scheme define selection must include a signature list: (define (name args...) body)",
        )?;
        if signature.kind != ExpressionKind::List || signature.delimiter != Some(Delimiter::Paren) {
            anyhow::bail!(
                "inline-function currently supports scheme procedure defines, not variable defines"
            );
        }
        let name = atom_child(signature, 0)
            .context("scheme define signature must start with a function name")?;
        let (params, _) = inline_parameter_names_from_children(
            dialect,
            input,
            InlineDefinitionKind::Function,
            &signature.children[1..],
        )?;
        (SymbolName::new(name.to_owned())?, params, false, 2)
    } else {
        let name =
            atom_child(&view, 1).context("function definition must include a symbol name")?;
        let params = view
            .children
            .get(2)
            .context("function definition must include a parameter list")?;
        let (params, accepts_other_keys) =
            inline_parameter_names(dialect, input, definition_kind, params)?;
        (
            SymbolName::new(name.to_owned())?,
            params,
            accepts_other_keys,
            3,
        )
    };

    let body_forms = effective_body_forms(dialect, &view.children[body_start..]);
    let body = match definition_kind {
        InlineDefinitionKind::Function => inline_function_body_view(body_forms)?,
        InlineDefinitionKind::Macro => {
            let [body] = body_forms else {
                anyhow::bail!(
                    "inline-function currently requires exactly one effective body expression; found {}",
                    body_forms.len()
                );
            };
            body.clone()
        }
    };

    Ok(InlineDefinition {
        name,
        params,
        body,
        kind: definition_kind,
        accepts_other_keys,
    })
}
