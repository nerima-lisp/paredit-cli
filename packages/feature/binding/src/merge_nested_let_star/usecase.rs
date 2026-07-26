//! Application facade for merging directly nested sequential `let*` forms.

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::dialect::Dialect;
use crate::domain::let_composition::{self, MergeNestedLetStarRequest as DomainRequest};
use crate::domain::sexpr::{ByteSpan, Path, SyntaxTree};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct MergeNestedLetStarRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct MergeNestedLetStarPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_merge_nested_let_star(
    request: MergeNestedLetStarRequest<'_>,
) -> Result<MergeNestedLetStarPlan> {
    let_composition::validate_dialect(request.dialect, "merge-nested-let-star")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let plan = let_composition::plan_merge_nested_let_star(DomainRequest {
        input: request.input,
        dialect: request.dialect,
        path: request.path.clone(),
    })?;
    Ok(MergeNestedLetStarPlan {
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
        for (dialect, prefix) in [(Dialect::CommonLisp, r"#\)"), (Dialect::EmacsLisp, r"?\)")] {
            let input = format!("{prefix} (let* ((x 1)) (let* ((y (+ x 1))) y))");
            let plan = plan_merge_nested_let_star(MergeNestedLetStarRequest {
                input: &input,
                dialect,
                path: "1".parse().expect("path"),
            })
            .expect("supported dialect");
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("rewritten input");
        }

        for dialect in [Dialect::Scheme, Dialect::Unknown] {
            let error = plan_merge_nested_let_star(MergeNestedLetStarRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");
            assert_eq!(
                error.to_string(),
                "merge-nested-let-star supports only Common Lisp and Emacs Lisp"
            );
        }
    }
}
