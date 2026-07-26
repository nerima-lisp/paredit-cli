use anyhow::Result;

use crate::domain::common_lisp::is_common_lisp_earmuffed_special_variable_name;
use crate::domain::dialect::Dialect;
use crate::domain::lexical_scope::value_capture;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, Path, SymbolName};

use super::super::syntax::{atom_text, view_at_span};
use super::super::{LetBindingReport, LetFormReport};
use super::bindings::let_binding_candidates;
use super::references::{fallback_reference_count, let_binding_reference_spans};

pub(super) fn analyze_let_form(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    path: &Path,
) -> Result<Option<LetFormReport>> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return Ok(None);
    }
    if view.children.len() < 2 {
        return Ok(None);
    }
    let Some(head) = atom_text(&view.children[0]) else {
        return Ok(None);
    };
    if dialect.let_binding_form_for_head(head).is_none()
        && !dialect.supports_inline_let_refactor_head(head)
    {
        return Ok(None);
    }

    let binding_form = &view.children[1];
    let (binding_style, candidates) = let_binding_candidates(dialect, binding_form)?;
    let body_count = view.children.len().saturating_sub(2);
    let single_binding = candidates.len() == 1;
    let inline_supported_by_inline_let =
        dialect.supports_inline_let_refactor_head(head) && single_binding && body_count > 0;
    let mut bindings = Vec::with_capacity(candidates.len());

    for candidate in &candidates {
        let symbol = SymbolName::new(candidate.name.clone());
        let (reference_count, reference_spans) = match &symbol {
            Ok(symbol) => {
                let spans = let_binding_reference_spans(
                    dialect,
                    input,
                    view,
                    binding_form,
                    &candidates,
                    candidate,
                    symbol,
                )?;
                (spans.len(), spans)
            }
            Err(_) => (
                view.children[2..]
                    .iter()
                    .map(|body| fallback_reference_count(body, &candidate.name))
                    .sum(),
                Vec::new(),
            ),
        };
        let captured = match (&symbol, view_at_span(binding_form, candidate.value_span)) {
            (Ok(symbol), Some(value_view)) => value_capture(
                dialect,
                input,
                view.span,
                symbol,
                value_view,
                &reference_spans,
            ),
            _ => Vec::new(),
        };
        let mut risks = Vec::new();
        if !single_binding {
            risks.push("multiple-bindings");
        }
        if symbol.is_err() {
            risks.push("unsupported-binding-name");
        }
        // Rebinding an earmuffed (`*name*`) special variable, the
        // near-universal Common Lisp convention for a `defvar`/
        // `defparameter`-declared dynamic variable, is meaningful purely
        // through its dynamic-scope side effect for the rest of the body's
        // dynamic extent — `(let ((*read-eval* nil)) (read stream))`
        // legitimately has zero lexical references to `*read-eval*`. A
        // lexical "is this name referenced" check cannot tell this apart
        // from genuine dead code, so flag it distinctly instead of as
        // `unused-binding`: acting on that report (deleting the binding)
        // can silently change program behavior — in the read-eval example,
        // reinstating an arbitrary-code-execution risk the binding exists
        // to close.
        if reference_count == 0
            && dialect == Dialect::CommonLisp
            && is_common_lisp_earmuffed_special_variable_name(&candidate.name)
        {
            risks.push("possible-dynamic-variable-rebind");
        } else if reference_count == 0 {
            risks.push("unused-binding");
        }
        if reference_count > 1 {
            risks.push("duplicate-evaluation");
        }
        if !captured.is_empty() {
            risks.push("capture");
        }
        if !inline_supported_by_inline_let {
            risks.push("unsupported-by-inline-let");
        }

        let sliced_value = candidate.value_span.slice(input);
        bindings.push(LetBindingReport {
            name: candidate.name.clone(),
            // A zero-width value_span means the binding had no explicit
            // value form (an implicit-nil binding); report its true value
            // instead of an empty string.
            value: if sliced_value.is_empty() {
                "nil".to_owned()
            } else {
                sliced_value.to_owned()
            },
            value_span: candidate.value_span,
            reference_count,
            can_inline_without_duplication: risks.is_empty(),
            risks,
        });
    }

    Ok(Some(LetFormReport {
        path: path.clone(),
        form: head.to_owned(),
        span: view.span,
        binding_style,
        body_count,
        inline_supported_by_inline_let,
        bindings,
    }))
}
