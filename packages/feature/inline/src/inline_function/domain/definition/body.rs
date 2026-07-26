use anyhow::{Context, Result};

use crate::domain::common_lisp::{CommonLispOperator, is_common_lisp_declaration_form};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView};

pub(super) fn unsupported_inline_function_definition_message(
    dialect: Dialect,
    head: &str,
) -> String {
    if matches!(dialect, Dialect::CommonLisp | Dialect::Unknown)
        && CommonLispOperator::from_head(head) == Some(CommonLispOperator::DefineSetfExpander)
    {
        return format!(
            "inline-function does not support definition head: {head} (setf expanders rewrite places, not ordinary call expressions)"
        );
    }

    format!("inline-function does not support definition head: {head}")
}

pub(super) fn inline_function_body_view(body_forms: &[ExpressionView]) -> Result<ExpressionView> {
    let [body] = body_forms else {
        let first = body_forms
            .first()
            .context("inline-function definition must include at least one body expression")?;
        let last = body_forms
            .last()
            .context("inline-function expected a non-empty effective body after validation")?;
        return Ok(ExpressionView {
            kind: ExpressionKind::Root,
            delimiter: None,
            reader_prefixes: Vec::new(),
            span: ByteSpan::new(first.span.start(), last.span.end()),
            content_span: ByteSpan::new(first.content_span.start(), last.content_span.end()),
            text: None,
            children: body_forms.to_vec(),
            symbol_offset: 0,
        });
    };
    Ok(body.clone())
}

pub(super) fn effective_body_forms(
    dialect: Dialect,
    body_forms: &[ExpressionView],
) -> &[ExpressionView] {
    if !dialect.supports_common_lisp_lambda_list_refactor_model() {
        return body_forms;
    }

    let mut start = 0usize;
    if body_forms.len().saturating_sub(start) > 1
        && body_forms[start].kind == ExpressionKind::Atom
        && body_forms[start]
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('"'))
    {
        start += 1;
    }

    while body_forms.len().saturating_sub(start) > 1
        && body_forms[start]
            .children
            .first()
            .and_then(super::super::syntax::atom_text)
            .is_some_and(is_common_lisp_declaration_form)
    {
        start += 1;
    }

    &body_forms[start..]
}
