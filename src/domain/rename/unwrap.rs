use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;
pub use crate::domain::rename::UnwrapFunctionCallsScope;
use crate::domain::sexpr::{ByteSpan, SymbolName, SyntaxTree};

mod call_site;
mod choose;
mod collect;

use collect::{collect_unwrap_all_call_sites, collect_unwrap_explicit_call_sites};

use super::selection::apply_byte_span_edits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnwrapFunctionCallSite {
    pub path: String,
    pub span: ByteSpan,
    pub replacement: String,
    pub text: String,
}

impl super::selection::SpannedCallSite for UnwrapFunctionCallSite {
    fn span(&self) -> ByteSpan {
        self.span
    }
}

#[derive(Debug, Clone)]
pub struct UnwrapFunctionCallsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub function: SymbolName,
    pub wrapper: SymbolName,
    pub scope: UnwrapFunctionCallsScope,
}

#[derive(Debug, Clone)]
pub struct UnwrapFunctionCallsPlan {
    pub dialect: Dialect,
    pub calls: Vec<UnwrapFunctionCallSite>,
    pub skipped_non_unary_wrapper: Vec<UnwrapFunctionCallSite>,
    pub skipped_nested: Vec<UnwrapFunctionCallSite>,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_unwrap_function_calls(
    request: UnwrapFunctionCallsRequest<'_>,
) -> Result<UnwrapFunctionCallsPlan> {
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
        Dialect::Unknown => anyhow::bail!("unwrap-function-calls requires a known dialect"),
    }

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse input")?;
    let (calls, skipped_non_unary_wrapper, skipped_nested) = match &request.scope {
        UnwrapFunctionCallsScope::AllCalls => collect_unwrap_all_call_sites(
            &tree,
            request.dialect,
            request.input,
            &request.function,
            &request.wrapper,
        )?,
        UnwrapFunctionCallsScope::ExplicitPaths(paths) => collect_unwrap_explicit_call_sites(
            &tree,
            request.dialect,
            request.input,
            paths,
            &request.function,
            &request.wrapper,
        )?,
    };
    let edits = calls
        .iter()
        .map(|site| (site.span, site.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("unwrapped output is not a valid S-expression document")?;

    Ok(UnwrapFunctionCallsPlan {
        dialect: request.dialect,
        calls,
        skipped_non_unary_wrapper,
        skipped_nested,
        changed: rewritten != request.input,
        rewritten,
    })
}
