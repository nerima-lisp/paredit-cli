use anyhow::{Context, Result, bail};

use crate::domain::dialect::Dialect;
use crate::domain::form_shape::duplicate_shape;
use crate::domain::mutation_safety::{
    reject_common_lisp_reader_conditionals, reject_overlapping_common_lisp_reader_time_forms,
};
use crate::domain::sexpr::{Path, SyntaxTree};

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

pub fn plan_replace_forms(request: ReplaceFormsRequest<'_>) -> Result<ReplaceFormsPlan> {
    ensure_supported_dialect(request.dialect)?;

    let input_tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("replace-forms input is not a valid S-expression document")?;
    anyhow::ensure!(
        &input_tree == request.tree,
        "replace-forms input does not match the source used to build the syntax tree"
    );

    let replacement_tree = SyntaxTree::parse_with_dialect(request.replacement, request.dialect)
        .context("--with must be a valid S-expression document")?;
    // The replacement becomes source code in the rewritten document, so it
    // must satisfy the same reader-time safety contract as the input tree.
    reject_common_lisp_reader_conditionals(&replacement_tree, request.dialect)?;
    anyhow::ensure!(
        replacement_tree.root_children().len() == 1,
        "--with must contain exactly one top-level S-expression"
    );
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
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("replace-forms output is not a valid S-expression document")?;

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

fn ensure_supported_dialect(dialect: Dialect) -> Result<()> {
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
        Dialect::Unknown => bail!("replace-forms requires a known dialect"),
    }
}
