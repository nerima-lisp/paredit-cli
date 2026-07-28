use crate::error::{CallBindingError, InlineError, InlineResult, InlineSelectionError};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
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

pub fn bind_inline_function_arguments(
    dialect: Dialect,
    params: &[InlineParameter],
    call: InlineFunctionCall,
    function_name: &SymbolName,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
) -> InlineResult<InlineArgumentBindings> {
    binding::bind_inline_function_arguments(
        dialect,
        params,
        call,
        function_name,
        accepts_other_keys,
        allow_drop_arguments,
    )
}

pub fn resolve_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    explicit_call_paths: Vec<Path>,
    all_calls: bool,
    definition_span: paredit_core_syntax::sexpr::ByteSpan,
    function_name: &SymbolName,
    command: &'static str,
) -> InlineResult<Vec<Path>> {
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

pub fn parse_inline_function_call(
    dialect: Dialect,
    view: ExpressionView,
    function_name: &SymbolName,
    input: &str,
) -> InlineResult<InlineFunctionCall> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return Err(InlineSelectionError::Shape {
            operation: "inline-function",
            problem: "call selection must be a function call list".to_owned(),
        }
        .into());
    }
    let head = atom_text(view.children.first().ok_or_else(|| {
        InlineError::from(InlineSelectionError::Shape {
            operation: "inline-function",
            problem: "call must not be empty".to_owned(),
        })
    })?)
    .ok_or_else(|| {
        InlineError::from(InlineSelectionError::Shape {
            operation: "inline-function",
            problem: "call must start with an atom".to_owned(),
        })
    })?;
    if !inline_function_symbol_reference_eq(dialect, head, function_name.as_str()) {
        return Err(InlineSelectionError::Shape {
            operation: "inline-function",
            problem: format!(
                "call head '{head}' does not match selected definition '{function_name}'"
            ),
        }
        .into());
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
    definition_span: paredit_core_syntax::sexpr::ByteSpan,
    function_name: &SymbolName,
    command: &'static str,
) -> InlineResult<()> {
    let discoverable_call_paths =
        discovery::discover_function_call_paths(tree, dialect, definition_span, function_name)?;
    for call_path in explicit_call_paths {
        let selection = tree.select_path(call_path)?;
        let view = selection.view();
        if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
            return Err(CallBindingError::CallPathNotACallList {
                command,
                path: call_path.to_string(),
            }
            .into());
        }

        let head = list_head(&view)
            .ok_or_else(|| {
                InlineError::from(InlineSelectionError::Shape {
                    operation: "inline-function",
                    problem: "call must not be empty".to_owned(),
                })
            })?
            .to_owned();
        if !inline_function_symbol_reference_eq(dialect, &head, function_name.as_str()) {
            return Err(CallBindingError::CallPathHeadMismatch {
                command,
                path: call_path.to_string(),
                head: head.to_string(),
                function: function_name.to_string(),
            }
            .into());
        }

        if !discoverable_call_paths.iter().any(|path| path == call_path) {
            return Err(CallBindingError::CallPathShadowed {
                command,
                path: call_path.to_string(),
            }
            .into());
        }
    }

    Ok(())
}

pub fn inline_function_symbol_reference_eq(dialect: Dialect, left: &str, right: &str) -> bool {
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

pub fn validate_or_resolve_function_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    explicit_call_paths: Vec<Path>,
    all_calls: bool,
    definition_span: paredit_core_syntax::sexpr::ByteSpan,
    function_name: &SymbolName,
    command: &'static str,
) -> InlineResult<Vec<Path>> {
    if all_calls && !explicit_call_paths.is_empty() {
        return Err(CallBindingError::AllCallsAndCallPath { command }.into());
    }

    if all_calls {
        let call_paths =
            discovery::discover_function_call_paths(tree, dialect, definition_span, function_name)?;
        if call_paths.is_empty() {
            return Err(CallBindingError::NoSameFileCalls {
                command,
                function: function_name.to_string(),
            }
            .into());
        }
        return Ok(call_paths);
    }

    if explicit_call_paths.is_empty() {
        return Err(CallBindingError::NoCallSelector { command }.into());
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
