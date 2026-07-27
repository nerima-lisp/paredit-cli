use paredit_core_edit::DocumentRefusal;

use crate::error::{RenameError, RenameResult};

pub use crate::rename::domain::ReplaceFunctionCallsScope;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SymbolName, SyntaxTree};

mod call_site;
mod collect;

use collect::{collect_all_replace_call_sites, collect_explicit_replace_call_sites};

use super::selection::apply_byte_span_edits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceFunctionCallSite {
    pub path: String,
    pub head_span: ByteSpan,
    pub span: ByteSpan,
    pub replacement: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ReplaceFunctionCallsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub from: SymbolName,
    pub to: SymbolName,
    pub scope: ReplaceFunctionCallsScope,
}

#[derive(Debug, Clone)]
pub struct ReplaceFunctionCallsPlan {
    pub dialect: Dialect,
    pub calls: Vec<ReplaceFunctionCallSite>,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_replace_function_calls(
    request: ReplaceFunctionCallsRequest<'_>,
) -> RenameResult<ReplaceFunctionCallsPlan> {
    match request.dialect {
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => {}
        Dialect::Unknown => {
            return Err(RenameError::RequiresKnownDialect {
                operation: "replace-function-calls",
            });
        }
    }

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let calls = match &request.scope {
        ReplaceFunctionCallsScope::AllCalls => collect_all_replace_call_sites(
            &tree,
            request.dialect,
            request.input,
            &request.from,
            &request.to,
        )?,
        ReplaceFunctionCallsScope::ExplicitPaths(paths) => collect_explicit_replace_call_sites(
            &tree,
            request.dialect,
            request.input,
            paths,
            &request.from,
            &request.to,
        )?,
    };
    let edits = calls
        .iter()
        .map(|site| (site.head_span, site.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "replace-function-calls",
            source,
        }
    })?;

    Ok(ReplaceFunctionCallsPlan {
        dialect: request.dialect,
        calls,
        changed: rewritten != request.input,
        rewritten,
    })
}
