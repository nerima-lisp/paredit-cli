//! Application facade for safely splitting a parallel `let`.

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::binding_index::BindingIndex;
use crate::domain::dialect::Dialect;
use crate::domain::let_composition::{self, SplitLetRequest as DomainRequest};
use crate::domain::sexpr::{ByteSpan, Path, SyntaxTree};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SplitLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    pub binding_index: usize,
}
#[derive(Debug, Clone)]
pub struct SplitLetPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_index: usize,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_split_let(request: SplitLetRequest<'_>) -> Result<SplitLetPlan> {
    let_composition::validate_dialect(request.dialect, "split-let")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let binding_index = BindingIndex::new(request.binding_index)?;
    let plan = let_composition::plan_split_let(DomainRequest {
        input: request.input,
        dialect: request.dialect,
        path: request.path.clone(),
        binding_index,
    })?;
    Ok(SplitLetPlan {
        dialect: plan.dialect,
        path: plan.path,
        form_span: plan.form_span,
        binding_index: plan.binding_index.get(),
        outer_binding_count: plan.outer_binding_count,
        inner_binding_count: plan.inner_binding_count,
        rewritten: plan.rewritten,
        changed: plan.changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dialect_before_parsing_and_uses_dialect_parser() {
        for (dialect, prefix) in [(Dialect::CommonLisp, r"#\)"), (Dialect::EmacsLisp, r"?\)")] {
            let input = format!("{prefix} (let ((x 1) (y 2)) (+ x y))");
            let plan = plan_split_let(SplitLetRequest {
                input: &input,
                dialect,
                path: "1".parse().expect("path"),
                binding_index: 1,
            })
            .expect("supported dialect");
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("rewritten input");
        }

        for dialect in [Dialect::Scheme, Dialect::Unknown] {
            let error = plan_split_let(SplitLetRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
                binding_index: 0,
            })
            .expect_err("unsupported dialect");
            assert_eq!(
                error.to_string(),
                "split-let supports only Common Lisp and Emacs Lisp"
            );
        }
    }
}
