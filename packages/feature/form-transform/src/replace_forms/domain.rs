use paredit_core_edit::DocumentRefusal;

use crate::error::{FormTransformResult, TransformDialectError, TransformSelectorError};

use paredit_core_edit::mutation_safety::{
    reject_common_lisp_reader_conditionals, reject_overlapping_common_lisp_reader_time_forms,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::form_shape::duplicate_shape;
use paredit_core_syntax::sexpr::{Path, SyntaxTree};

mod rewrite;
#[cfg(test)]
mod tests;
mod types;
mod validation;

use rewrite::rewrite_replace_targets;
pub use types::{ReplaceFormsPlan, ReplaceFormsRequest, ReplaceFormsTarget};
use validation::{
    collect_replace_targets, ensure_same_shape_when_required, original_shape_for_targets,
};

pub fn plan_replace_forms(
    request: ReplaceFormsRequest<'_>,
) -> FormTransformResult<ReplaceFormsPlan> {
    ensure_supported_dialect(request.dialect)?;

    let input_tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "replace-forms",
                source,
            }
        })?;
    if &input_tree != request.tree {
        return Err(TransformSelectorError::InputDoesNotMatchTree.into());
    }

    let replacement_tree = SyntaxTree::parse_with_dialect(request.replacement, request.dialect)
        .map_err(|source| DocumentRefusal::InputNotAnSexprDocument {
            operation: "--with",
            source,
        })?;
    // The replacement becomes source code in the rewritten document, so it
    // must satisfy the same reader-time safety contract as the input tree.
    reject_common_lisp_reader_conditionals(&replacement_tree, request.dialect)?;
    if replacement_tree.root_children().len() != 1 {
        return Err(TransformSelectorError::WithNotOneForm.into());
    }
    let replacement_view = replacement_tree.select_path(&Path::root_child(0))?.view();
    let replacement_shape = duplicate_shape(&replacement_view, true);

    let targets = collect_replace_targets(request.tree, &request.paths)?;
    reject_overlapping_common_lisp_reader_time_forms(
        request.tree,
        request.dialect,
        targets.iter().map(|target| target.span),
    )?;
    let original_shape = original_shape_for_targets(&targets);
    ensure_same_shape_when_required(
        &targets,
        original_shape.as_ref(),
        request.require_same_shape,
    )?;

    let rewritten = rewrite_replace_targets(request.input, &targets, request.replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "replace-forms",
            source,
        }
    })?;

    let changed = rewritten != request.input;
    Ok(ReplaceFormsPlan {
        targets,
        replacement: request.replacement.to_owned(),
        replacement_shape,
        require_same_shape: request.require_same_shape,
        original_shape,
        changed,
        rewritten,
    })
}

fn ensure_supported_dialect(dialect: Dialect) -> FormTransformResult<()> {
    match dialect {
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => Ok(()),
        Dialect::Unknown => Err(TransformDialectError::RequiresKnown {
            operation: "replace-forms",
        }
        .into()),
    }
}
