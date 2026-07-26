use std::collections::HashSet;

use anyhow::{Context, Result};

use super::ReplaceFormsTarget;
use crate::domain::form_shape::{FormShape, duplicate_shape};
use crate::domain::sexpr::{Path, SyntaxTree};

pub(super) fn collect_replace_targets(
    tree: &SyntaxTree,
    paths: &[Path],
) -> Result<Vec<ReplaceFormsTarget>> {
    let mut seen_paths = HashSet::<Path>::new();
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        let path_key = path.to_string();
        anyhow::ensure!(
            seen_paths.insert(path.clone()),
            "duplicate --path: {path_key}"
        );
        let selection = tree
            .select_path(path)
            .with_context(|| format!("invalid --path {path_key}"))?;
        let view = selection.view();
        targets.push(ReplaceFormsTarget {
            form_path: path.clone(),
            span: selection.span(),
            shape: duplicate_shape(&view, true),
            text: selection.text().to_owned(),
        });
    }

    ensure_non_overlapping_replace_targets(&targets)?;
    Ok(targets)
}

pub(super) fn original_shape_for_targets(targets: &[ReplaceFormsTarget]) -> Option<FormShape> {
    targets.first().map(|target| target.shape.clone())
}

pub(super) fn ensure_same_shape_when_required(
    targets: &[ReplaceFormsTarget],
    original_shape: Option<&FormShape>,
    require_same_shape: bool,
) -> Result<()> {
    if !require_same_shape {
        return Ok(());
    }

    let Some(expected_shape) = original_shape else {
        anyhow::bail!("replace-forms requires at least one --path");
    };
    for target in targets {
        anyhow::ensure!(
            &target.shape == expected_shape,
            "replace-forms --require-same-shape expected all selected forms to share shape; {} differs",
            target.form_path
        );
    }

    Ok(())
}

fn ensure_non_overlapping_replace_targets(targets: &[ReplaceFormsTarget]) -> Result<()> {
    let mut ordered = targets.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|target| target.span.start().get());

    for pair in ordered.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if left.span.end().get() > right.span.start().get() {
            anyhow::bail!(
                "replace-forms paths must not overlap: {} and {}",
                left.form_path,
                right.form_path
            );
        }
    }

    Ok(())
}
