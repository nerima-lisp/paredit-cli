//! Shared reference scanning rules for definition reachability.

use std::collections::HashSet;

use crate::domain::common_lisp::{
    CommonLispLocalCallableForm, CommonLispPackageDeclarationForm, common_lisp_local_callable_form,
    common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
    common_lisp_symbol_reference_needle, is_local_callable_bound,
    local_callable_binding_body_scope, local_callable_body_scope,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::{atom_symbol_text, atom_text};
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SymbolName};

pub(crate) fn collect_reference_needles(view: &ExpressionView, output: &mut HashSet<String>) {
    if view.kind == ExpressionKind::Atom {
        if let Some(text) = atom_symbol_text(view) {
            output.insert(common_lisp_symbol_reference_needle(text));
        }
        return;
    }
    for child in &view.children {
        collect_reference_needles(child, output);
    }
}

pub(crate) fn collect_package_form_spans(
    dialect: Dialect,
    view: &ExpressionView,
    output: &mut Vec<ByteSpan>,
) {
    if let Some(head) = list_head(view) {
        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::Defpackage)
        {
            output.push(view.span);
            return;
        }
    }

    for child in &view.children {
        collect_package_form_spans(dialect, child, output);
    }
}

pub(crate) fn collect_function_quote_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
) {
    collect_function_quote_references_from_view(dialect, view, symbol, &[], output);
}

fn collect_function_quote_references_from_view(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    local_callables: &[String],
    output: &mut Vec<ByteSpan>,
) {
    if let Some(head) = list_head(view) {
        if let Some(form) = common_lisp_local_callable_form(dialect, head) {
            collect_local_callable_function_quote_references(
                dialect,
                view,
                symbol,
                local_callables,
                form,
                output,
            );
            return;
        }
    }

    if let Some(target) = callable_reference_target(view) {
        if callable_reference_matches(dialect, target, symbol)
            && !is_local_callable_bound(local_callables, symbol.as_str())
        {
            output.push(target.span);
            return;
        }
    }

    for child in &view.children {
        collect_function_quote_references_from_view(
            dialect,
            child,
            symbol,
            local_callables,
            output,
        );
    }
}

fn collect_local_callable_function_quote_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    local_callables: &[String],
    form: CommonLispLocalCallableForm,
    output: &mut Vec<ByteSpan>,
) {
    let body_scope = local_callable_body_scope(local_callables, view);

    if let Some(bindings) = view.children.get(1) {
        let binding_body_scope =
            local_callable_binding_body_scope(form, local_callables, &body_scope);
        for binding in &bindings.children {
            for child in binding.children.iter().skip(2) {
                collect_function_quote_references_from_view(
                    dialect,
                    child,
                    symbol,
                    binding_body_scope,
                    output,
                );
            }
        }
    }

    for child in view.children.iter().skip(2) {
        collect_function_quote_references_from_view(dialect, child, symbol, &body_scope, output);
    }
}

fn callable_reference_target(view: &ExpressionView) -> Option<&ExpressionView> {
    if view.reader_prefixes.contains(&ReaderPrefix::Function) {
        return Some(view);
    }

    let target = callable_accessor_target(view)?;
    Some(target)
}

fn callable_accessor_target(view: &ExpressionView) -> Option<&ExpressionView> {
    (view.kind == ExpressionKind::List).then_some(())?;
    let head = atom_text(view.children.first()?)?;
    let target = view.children.get(1)?;

    matches!(
        common_lisp_callable_accessor_kind(head),
        Some(CallableAccessorKind::Function | CallableAccessorKind::QuotedFunction)
    )
    .then_some(target)
}

fn callable_reference_matches(
    dialect: Dialect,
    candidate: &ExpressionView,
    symbol: &SymbolName,
) -> bool {
    atom_symbol_text(candidate)
        .is_some_and(|text| function_quote_symbol_matches(dialect, text, symbol.as_str()))
        || setf_callable_name_view(candidate).is_some_and(|name| {
            atom_symbol_text(name)
                .is_some_and(|text| function_quote_symbol_matches(dialect, text, symbol.as_str()))
        })
}

fn setf_callable_name_view(view: &ExpressionView) -> Option<&ExpressionView> {
    (view.kind == ExpressionKind::List).then_some(())?;
    let head = view.children.first().and_then(atom_text)?;
    common_lisp_operator_head_eq(head, "setf").then_some(())?;
    view.children
        .get(1)
        .filter(|name| name.kind == ExpressionKind::Atom)
}

fn common_lisp_callable_accessor_kind(head: &str) -> Option<CallableAccessorKind> {
    if common_lisp_operator_head_eq(head, "function") {
        return Some(CallableAccessorKind::Function);
    }

    if matches!(
        head,
        "macro-function" | "compiler-macro-function" | "symbol-function" | "fdefinition"
    ) || common_lisp_operator_head_eq(head, "macro-function")
        || common_lisp_operator_head_eq(head, "compiler-macro-function")
        || common_lisp_operator_head_eq(head, "symbol-function")
        || common_lisp_operator_head_eq(head, "fdefinition")
    {
        return Some(CallableAccessorKind::QuotedFunction);
    }

    None
}

#[derive(Clone, Copy)]
enum CallableAccessorKind {
    Function,
    QuotedFunction,
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List)
        .then_some(view.children.first())
        .flatten()
        .and_then(atom_text)
}

fn function_quote_symbol_matches(dialect: Dialect, candidate: &str, symbol: &str) -> bool {
    match dialect {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(candidate, symbol),
        _ => candidate == symbol,
    }
}

pub(crate) fn collect_quoted_data_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
) {
    if view
        .reader_prefixes
        .iter()
        .any(|prefix| matches!(prefix, ReaderPrefix::Quote | ReaderPrefix::Quasiquote))
    {
        collect_atoms_in_quoted_region(dialect, view, symbol, output);
        return;
    }

    if callable_accessor_target(view).is_some() {
        for (index, child) in view.children.iter().enumerate() {
            if index == 1 {
                continue;
            }
            collect_quoted_data_references(dialect, child, symbol, output);
        }
        return;
    }

    for child in &view.children {
        collect_quoted_data_references(dialect, child, symbol, output);
    }
}

fn collect_atoms_in_quoted_region(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
) {
    if view.kind == ExpressionKind::Atom {
        if atom_symbol_text(view)
            .is_some_and(|text| function_quote_symbol_matches(dialect, text, symbol.as_str()))
        {
            output.push(view.span);
        }
        return;
    }

    for child in &view.children {
        collect_atoms_in_quoted_region(dialect, child, symbol, output);
    }
}

pub(crate) fn collect_symbol_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    source: &str,
    output: &mut Vec<ByteSpan>,
) {
    crate::domain::lexical_scope::collect_unshadowed_symbol_references(
        dialect, view, symbol, source, output,
    );
    collect_function_quote_references(dialect, view, symbol, output);
    collect_quoted_data_references(dialect, view, symbol, output);
}
