use paredit_core_edit::DocumentRefusal;

use crate::error::{CallSiteError, RenameError, RenameResult};

pub use crate::rename::domain::WrapFunctionCallsScope;
use crate::rename::domain::call_identity::call_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

mod call_site;
mod choose;
mod collect;

use collect::{collect_wrap_all_call_sites, collect_wrap_explicit_call_sites};

use super::selection::apply_byte_span_edits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapFunctionCallSite {
    pub path: String,
    pub span: ByteSpan,
    pub replacement: String,
    pub text: String,
}

impl super::selection::SpannedCallSite for WrapFunctionCallSite {
    fn span(&self) -> ByteSpan {
        self.span
    }
}

#[derive(Debug, Clone)]
pub struct WrapFunctionCallsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub function: SymbolName,
    pub wrapper: SymbolName,
    pub wrapper_template: Option<String>,
    pub scope: WrapFunctionCallsScope,
}

#[derive(Debug, Clone)]
pub struct WrapFunctionCallsPlan {
    pub dialect: Dialect,
    pub calls: Vec<WrapFunctionCallSite>,
    pub skipped_already_wrapped: Vec<WrapFunctionCallSite>,
    pub skipped_nested: Vec<WrapFunctionCallSite>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct WrapFunctionCallTemplate {
    source: String,
    placeholder_span: ByteSpan,
}

impl WrapFunctionCallTemplate {
    fn parse(source: String, dialect: Dialect, wrapper: &SymbolName) -> RenameResult<Self> {
        let tree = SyntaxTree::parse_with_dialect(&source, dialect)
            .map_err(|source| CallSiteError::WrapperTemplateDoesNotParse { source })?;
        if tree.root_children().len() != 1 {
            return Err(CallSiteError::WrapperTemplateNotOneForm.into());
        }

        let root = tree.select_path(&Path::root_child(0))?.view();
        let head = crate::rename::domain::selection::list_head(&root)
            .ok_or(CallSiteError::WrapperTemplateNotAList)?;
        if !call_reference_eq(dialect, head, wrapper.as_str()) {
            return Err(CallSiteError::WrapperTemplateHeadMismatch {
                wrapper: wrapper.as_str().to_owned(),
            }
            .into());
        }

        let mut placeholders = Vec::new();
        collect_template_placeholders(&root, &mut placeholders);
        if placeholders.len() != 1 {
            return Err(CallSiteError::WrapperTemplateNotOnePlaceholder.into());
        }

        Ok(Self {
            source,
            placeholder_span: placeholders[0],
        })
    }

    pub fn apply(&self, call_text: &str) -> RenameResult<String> {
        apply_byte_span_edits(
            &self.source,
            vec![(self.placeholder_span, call_text.to_owned())],
        )
    }
}

fn collect_template_placeholders(view: &ExpressionView, output: &mut Vec<ByteSpan>) {
    if view.kind == ExpressionKind::Atom && view.text.as_deref() == Some("_") {
        output.push(view.span);
        return;
    }
    for child in &view.children {
        collect_template_placeholders(child, output);
    }
}

pub fn plan_wrap_function_calls(
    request: WrapFunctionCallsRequest<'_>,
) -> RenameResult<WrapFunctionCallsPlan> {
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
                operation: "wrap-function-calls",
            });
        }
    }

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let template = request
        .wrapper_template
        .map(|source| WrapFunctionCallTemplate::parse(source, request.dialect, &request.wrapper))
        .transpose()?;
    let (calls, skipped_already_wrapped, skipped_nested) = match &request.scope {
        WrapFunctionCallsScope::AllCalls => collect_wrap_all_call_sites(
            &tree,
            request.dialect,
            request.input,
            &request.function,
            &request.wrapper,
            template.as_ref(),
        )?,
        WrapFunctionCallsScope::ExplicitPaths(paths) => collect_wrap_explicit_call_sites(
            &tree,
            request.dialect,
            request.input,
            paths,
            &request.function,
            &request.wrapper,
            template.as_ref(),
        )?,
    };
    let edits = calls
        .iter()
        .map(|site| (site.span, site.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "wrapped",
            source,
        }
    })?;

    Ok(WrapFunctionCallsPlan {
        dialect: request.dialect,
        calls,
        skipped_already_wrapped,
        skipped_nested,
        changed: rewritten != request.input,
        rewritten,
    })
}
