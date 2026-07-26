mod bindings;
mod forms;
mod patterns;
mod semantic;
mod symbols;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_eq, is_common_lisp_declaration_form,
};
use crate::domain::dialect::{Dialect, ExtractFunctionOperation, VerifiedSemanticPolicy};
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, ReaderPrefix};

use super::syntax::{atom_text, list_head};
use forms::collect_inferred_extract_function_special_form;
use symbols::is_extract_function_param_candidate;

pub(super) type ExtractFunctionSemantic = VerifiedSemanticPolicy<ExtractFunctionOperation>;

pub(super) fn infer_extract_function_params(
    semantic: ExtractFunctionSemantic,
    selection: &ExpressionView,
    explicit_params: &[String],
) -> Vec<String> {
    let mut params = Vec::new();
    collect_inferred_extract_function_params(
        semantic,
        selection,
        false,
        explicit_params,
        &Vec::new(),
        &mut params,
    );
    params
}

pub(super) fn extract_function_param_name_eq(
    semantic: ExtractFunctionSemantic,
    left: &str,
    right: &str,
) -> bool {
    match semantic.dialect() {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(left, right),
        _ => semantic.identifiers_equal(left, right),
    }
}

fn collect_inferred_extract_function_params(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    is_call_head: bool,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    if let Some(text) = atom_text(view) {
        if view.reader_prefixes.contains(&ReaderPrefix::Function) {
            return;
        }

        if !is_call_head
            && is_extract_function_param_candidate(text)
            && !explicit_params
                .iter()
                .any(|param| extract_function_param_name_eq(semantic, param, text))
            && !bound_params
                .iter()
                .any(|param| extract_function_param_name_eq(semantic, param, text))
            && !params
                .iter()
                .any(|param| extract_function_param_name_eq(semantic, param, text))
        {
            params.push(text.to_owned());
        }
        return;
    }

    if semantic.dialect() == Dialect::CommonLisp
        && view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
    {
        if let Some(head) = list_head(view) {
            if is_common_lisp_declaration_form(head) {
                return;
            }
        }
    }

    if semantic::collect_inferred_extract_function_semantic_form(
        semantic,
        view,
        explicit_params,
        bound_params,
        params,
    ) {
        return;
    }

    if collect_inferred_extract_function_special_form(
        semantic,
        view,
        explicit_params,
        bound_params,
        params,
    ) {
        return;
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_inferred_extract_function_params(
            semantic,
            child,
            view.kind == ExpressionKind::List
                && view.delimiter == Some(Delimiter::Paren)
                && index == 0,
            explicit_params,
            bound_params,
            params,
        );
    }
}
