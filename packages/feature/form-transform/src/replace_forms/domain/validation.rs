use std::collections::HashSet;

use crate::error::{FormTransformResult, TransformSelectorError};

use super::ReplaceFormsTarget;
use paredit_core_syntax::form_shape::{FormShape, duplicate_shape};
use paredit_core_syntax::sexpr::{Path, SyntaxTree};

pub fn collect_replace_targets(
    tree: &SyntaxTree,
    paths: &[Path],
) -> FormTransformResult<Vec<ReplaceFormsTarget>> {
    let mut seen_paths = HashSet::<Path>::new();
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        let path_key = path.to_string();
        if !seen_paths.insert(path.clone()) {
            return Err(TransformSelectorError::DuplicatePath {
                path: path_key.to_string(),
            }
            .into());
        }
        let selection =
            tree.select_path(path)
                .map_err(|_| TransformSelectorError::InvalidPath {
                    path: path_key.to_string(),
                })?;
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

pub fn original_shape_for_targets(targets: &[ReplaceFormsTarget]) -> Option<FormShape> {
    targets.first().map(|target| target.shape.clone())
}

pub fn ensure_same_shape_when_required(
    targets: &[ReplaceFormsTarget],
    original_shape: Option<&FormShape>,
    require_same_shape: bool,
) -> FormTransformResult<()> {
    if !require_same_shape {
        return Ok(());
    }

    let Some(expected_shape) = original_shape else {
        return Err(TransformSelectorError::ReplaceFormsNeedsPath.into());
    };
    for target in targets {
        if &target.shape != expected_shape {
            return Err(TransformSelectorError::ShapeMismatch {
                path: target.form_path.to_string(),
            }
            .into());
        }
    }

    Ok(())
}

fn ensure_non_overlapping_replace_targets(
    targets: &[ReplaceFormsTarget],
) -> FormTransformResult<()> {
    let mut ordered = targets.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|target| target.span.start().get());

    for pair in ordered.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if left.span.end().get() > right.span.start().get() {
            return Err(TransformSelectorError::OverlappingPaths {
                first: left.form_path.to_string(),
                second: right.form_path.to_string(),
            }
            .into());
        }
    }

    Ok(())
}
