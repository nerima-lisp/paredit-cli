//! Application facade for inlining an immediately invoked Common Lisp lambda.

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::dialect::Dialect;
use crate::domain::inline_lambda::{self, Request as DomainRequest};
use crate::domain::sexpr::{ByteSpan, Path, SymbolName, SyntaxTree};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct InlineLambdaRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct InlineLambdaBindingPlan {
    pub name: SymbolName,
    pub argument: String,
}
#[derive(Debug, Clone)]
pub struct InlineLambdaPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub call_span: ByteSpan,
    pub lambda_span: ByteSpan,
    pub bindings: Vec<InlineLambdaBindingPlan>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_inline_lambda(request: InlineLambdaRequest<'_>) -> Result<InlineLambdaPlan> {
    inline_lambda::validate_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let plan = inline_lambda::plan(DomainRequest {
        input: request.input,
        dialect: request.dialect,
        path: request.path.clone(),
    })?;
    Ok(InlineLambdaPlan {
        dialect: plan.dialect,
        path: plan.path,
        call_span: plan.call_span,
        lambda_span: plan.lambda_span,
        bindings: plan
            .bindings
            .into_iter()
            .map(|binding| InlineLambdaBindingPlan {
                name: binding.name,
                argument: binding.argument,
            })
            .collect(),
        replacement: plan.replacement,
        rewritten: plan.rewritten,
        changed: plan.changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dialect_before_parsing_and_uses_dialect_parser() {
        let plan = plan_inline_lambda(InlineLambdaRequest {
            input: r"#\) ((lambda (x) x) 1)",
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("Common Lisp");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten input");

        for dialect in [Dialect::EmacsLisp, Dialect::Unknown] {
            let error = plan_inline_lambda(InlineLambdaRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");
            assert_eq!(
                error.to_string(),
                "inline-lambda currently supports only Common Lisp"
            );
        }
    }
}
