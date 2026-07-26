use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::patterns::extract_function_pattern_names;

#[derive(Debug)]
pub(super) struct ExtractFunctionBindingEntry {
    pub(super) names: Vec<String>,
    pub(super) value: Option<ExpressionView>,
}

pub(super) fn extract_function_binding_entries(
    semantic: super::ExtractFunctionSemantic,
    binding_form: &ExpressionView,
) -> Option<Vec<ExtractFunctionBindingEntry>> {
    match binding_form.delimiter {
        Some(Delimiter::Bracket) => {
            if binding_form.children.len() % 2 != 0 {
                return None;
            }
            Some(
                binding_form
                    .children
                    .chunks_exact(2)
                    .map(|pair| ExtractFunctionBindingEntry {
                        names: extract_function_pattern_names(semantic, &pair[0]),
                        value: Some(pair[1].clone()),
                    })
                    .collect(),
            )
        }
        Some(Delimiter::Paren) => Some(
            binding_form
                .children
                .iter()
                .map(|pair| {
                    if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren)
                    {
                        if pair.kind != ExpressionKind::Atom {
                            return None;
                        }
                        return Some(ExtractFunctionBindingEntry {
                            names: extract_function_pattern_names(semantic, pair),
                            value: None,
                        });
                    }
                    if pair.children.is_empty() || pair.children.len() > 2 {
                        return None;
                    }
                    Some(ExtractFunctionBindingEntry {
                        names: extract_function_pattern_names(semantic, &pair.children[0]),
                        value: pair.children.get(1).cloned(),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => None,
    }
}
