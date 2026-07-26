//! Application facade for merging directly nested Common Lisp `flet` forms.

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::dialect::Dialect;
use crate::domain::flet_composition::{self, Request as DomainRequest};
use crate::domain::sexpr::{ByteSpan, Path, SyntaxTree};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct MergeNestedFletRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct MergeNestedFletPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_merge_nested_flet(request: MergeNestedFletRequest<'_>) -> Result<MergeNestedFletPlan> {
    flet_composition::validate_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let plan = flet_composition::plan(DomainRequest {
        input: request.input,
        dialect: request.dialect,
        path: request.path.clone(),
    })?;
    Ok(MergeNestedFletPlan {
        dialect: plan.dialect,
        path: plan.path,
        form_span: plan.form_span,
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
        let plan = plan_merge_nested_flet(MergeNestedFletRequest {
            input: r"#\) (flet ((left () 1)) (flet ((right () 2)) (+ (left) (right))))",
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("Common Lisp");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten input");

        for dialect in [Dialect::EmacsLisp, Dialect::Unknown] {
            let error = plan_merge_nested_flet(MergeNestedFletRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");
            assert_eq!(
                error.to_string(),
                "merge-nested-flet supports only Common Lisp"
            );
        }
    }
}
