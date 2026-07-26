use anyhow::{Context, Result};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

mod binding;
mod destructure;
mod discovery;
mod keyword_args;
mod types;

use super::definition::InlineParameter;
use super::syntax::{atom_text, list_head};
use types::{InlineArgumentBindings, InlineFunctionCall};

pub(super) fn bind_inline_function_arguments(
    dialect: Dialect,
    params: &[InlineParameter],
    call: InlineFunctionCall,
    function_name: &SymbolName,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
) -> Result<InlineArgumentBindings> {
    binding::bind_inline_function_arguments(
        dialect,
        params,
        call,
        function_name,
        accepts_other_keys,
        allow_drop_arguments,
    )
}

pub(super) fn resolve_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    explicit_call_paths: Vec<Path>,
    all_calls: bool,
    definition_span: crate::domain::sexpr::ByteSpan,
    function_name: &SymbolName,
    command: &str,
) -> Result<Vec<Path>> {
    validate_or_resolve_function_call_paths(
        tree,
        dialect,
        explicit_call_paths,
        all_calls,
        definition_span,
        function_name,
        command,
    )
}

pub(super) fn parse_inline_function_call(
    dialect: Dialect,
    view: ExpressionView,
    function_name: &SymbolName,
    input: &str,
) -> Result<InlineFunctionCall> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("inline-function call selection must be a function call list");
    }
    let head = atom_text(
        view.children
            .first()
            .context("inline-function call must not be empty")?,
    )
    .context("inline-function call must start with an atom")?;
    if !inline_function_symbol_reference_eq(dialect, head, function_name.as_str()) {
        anyhow::bail!(
            "inline-function call head '{head}' does not match selected definition '{function_name}'"
        );
    }

    Ok(InlineFunctionCall {
        raw_args: view.children[1..]
            .iter()
            .map(|child| child.span.slice(input).to_owned())
            .collect(),
        whole_call: view.span.slice(input).to_owned(),
    })
}

fn validate_explicit_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    explicit_call_paths: &[Path],
    definition_span: crate::domain::sexpr::ByteSpan,
    function_name: &SymbolName,
    command: &str,
) -> Result<()> {
    let discoverable_call_paths =
        discovery::discover_function_call_paths(tree, dialect, definition_span, function_name)?;
    for call_path in explicit_call_paths {
        let selection = tree.select_path(call_path)?;
        let view = selection.view();
        if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
            anyhow::bail!("{command} --call-path {call_path} must select a function call list");
        }

        let head = list_head(&view)
            .context("inline-function call must not be empty")?
            .to_owned();
        if !inline_function_symbol_reference_eq(dialect, &head, function_name.as_str()) {
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

pub(super) fn inline_function_symbol_reference_eq(
    dialect: Dialect,
    left: &str,
    right: &str,
) -> bool {
    match dialect {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(left, right),
        Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => left == right,
        Dialect::Unknown => false,
    }
}

pub(super) fn validate_or_resolve_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    explicit_call_paths: Vec<Path>,
    all_calls: bool,
    definition_span: crate::domain::sexpr::ByteSpan,
    function_name: &SymbolName,
    command: &str,
) -> Result<Vec<Path>> {
    if all_calls && !explicit_call_paths.is_empty() {
        anyhow::bail!("{command} accepts either --all-calls or repeated --call-path, not both");
    }

    if all_calls {
        let call_paths =
            discovery::discover_function_call_paths(tree, dialect, definition_span, function_name)?;
        if call_paths.is_empty() {
            anyhow::bail!(
                "{command} --all-calls found no same-file calls for {function_name}"
            );
        }
        return Ok(call_paths);
    }

    if explicit_call_paths.is_empty() {
        anyhow::bail!("{command} requires at least one --call-path or --all-calls");
    }

    validate_explicit_function_call_paths(
        tree,
        dialect,
        &explicit_call_paths,
        definition_span,
        function_name,
        command,
    )?;

    Ok(explicit_call_paths)
}
