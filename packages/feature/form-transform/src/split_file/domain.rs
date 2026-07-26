//! Use case for splitting selected top-level definitions into another file.

use anyhow::{Context, Result};

use crate::domain::definition::definition_shape;
use crate::domain::mutation_safety::reject_overlapping_common_lisp_reader_time_forms;
use crate::domain::sexpr::{Path, SyntaxTree};

mod item;
mod rewrite;
mod syntax;
#[cfg(test)]
mod tests;
mod types;

use item::{build_split_file_item, package_context_before_top_level};
use rewrite::{append_top_level_definitions, ensure_non_overlapping_spans, replace_byte_span};
use syntax::list_head;
pub use types::{SplitFileDefinition, SplitFileItem, SplitFilePlan, SplitFileRequest};

pub fn plan_split_file(request: SplitFileRequest<'_>) -> Result<SplitFilePlan> {
    if request.paths.is_empty() && request.names.is_empty() && request.categories.is_empty() {
        anyhow::bail!("split-file requires at least one --path, --name, or --kind selector");
    }

    let from_tree = SyntaxTree::parse_with_dialect(request.from_input, request.from_dialect)
        .with_context(|| format!("failed to parse {}", request.from_file.display()))?;
    let to_tree = SyntaxTree::parse_with_dialect(request.to_input, request.to_dialect)
        .with_context(|| {
            format!(
                "destination file is not a valid S-expression document: {}",
                request.to_file.display()
            )
        })?;

    let mut seen_paths = std::collections::BTreeSet::new();
    let mut selected_paths = std::collections::BTreeMap::new();
    for path in &request.paths {
        if !seen_paths.insert(path.to_string()) {
            anyhow::bail!("duplicate split-file path: {path}");
        }

        let target_index = top_level_path_index(path, "split-file")?;
        if target_index >= from_tree.root_children().len() {
            anyhow::bail!("top-level path {path} is out of range");
        }

        selected_paths.insert(target_index, path.clone());
    }

    let requested_names = request
        .names
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let requested_categories = request
        .categories
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let mut matched_names = std::collections::BTreeSet::new();
    let mut matched_categories = std::collections::BTreeSet::new();

    if !requested_names.is_empty() || !requested_categories.is_empty() {
        for target_index in 0..from_tree.root_children().len() {
            let path = Path::root_child(target_index);
            let selection = from_tree.select_path(&path)?;
            let view = selection.view();
            let Some(head) = list_head(&view) else {
                continue;
            };
            let Some(shape) = definition_shape(request.from_dialect, &view, head) else {
                continue;
            };
            let name = shape.name(&view);
            let name_matches = name
                .map(|name| requested_names.contains(name))
                .unwrap_or(false);
            let category_matches = requested_categories.contains(&shape.category);

            if name_matches || category_matches {
                selected_paths.entry(target_index).or_insert(path);
                if let Some(name) = name.filter(|name| requested_names.contains(*name)) {
                    matched_names.insert(name.to_owned());
                }
                if requested_categories.contains(&shape.category) {
                    matched_categories.insert(shape.category);
                }
            }
        }
    }

    for name in &requested_names {
        if !matched_names.contains(name) {
            anyhow::bail!("split-file --name did not match a top-level definition: {name}");
        }
    }
    for category in &requested_categories {
        if !matched_categories.contains(category) {
            anyhow::bail!(
                "split-file --kind did not match any top-level definitions: {}",
                category.label()
            );
        }
    }

    let mut items = Vec::with_capacity(selected_paths.len());
    for (target_index, path) in selected_paths {
        let item = build_split_file_item(
            &from_tree,
            request.from_input,
            request.from_dialect,
            path,
            target_index,
        )?;
        items.push(item);
    }

    if items.is_empty() {
        anyhow::bail!("split-file selectors did not match any top-level definitions");
    }

    items.sort_by_key(|item| item.span.start().get());
    ensure_non_overlapping_spans(items.iter().map(|item| item.span))?;
    reject_overlapping_common_lisp_reader_time_forms(
        &from_tree,
        request.from_dialect,
        items.iter().map(|item| item.removal_span),
    )?;

    let destination_is_common_lisp =
        request.to_dialect == crate::domain::dialect::Dialect::CommonLisp;
    let mut running_package = if destination_is_common_lisp {
        package_context_before_top_level(
            &to_tree,
            request.to_dialect,
            to_tree.root_children().len(),
        )?
    } else {
        None
    };
    let definition_texts = items
        .iter()
        .map(|item| match &item.definition.package {
            Some(package)
                if destination_is_common_lisp
                    && running_package.as_deref() != Some(package.as_str()) =>
            {
                running_package = Some(package.clone());
                format!("(in-package {package})\n\n{}", item.definition_text)
            }
            _ => item.definition_text.clone(),
        })
        .collect::<Vec<_>>();
    // `item.removal_span` already starts at the boundary that hands the
    // *next* sibling's leading trivia back to it and ends exactly at this
    // definition's own end, so removing it verbatim leaves the original gap
    // after the definition as the new separator. Absorbing more trailing
    // whitespace here would glue the previous definition onto whatever
    // follows.
    let mut from_rewritten = request.from_input.to_owned();
    for item in items.iter_mut().rev() {
        from_rewritten = replace_byte_span(&from_rewritten, item.removal_span, "");
    }
    let to_rewritten = append_top_level_definitions(request.to_input, &definition_texts);

    SyntaxTree::parse_with_dialect(&from_rewritten, request.from_dialect).with_context(|| {
        format!(
            "source file would become invalid after splitting definitions: {}",
            request.from_file.display()
        )
    })?;
    SyntaxTree::parse_with_dialect(&to_rewritten, request.to_dialect).with_context(|| {
        format!(
            "destination file would become invalid after receiving definitions: {}",
            request.to_file.display()
        )
    })?;

    let changed = from_rewritten != request.from_input || to_rewritten != request.to_input;
    let written = request.write && changed;

    Ok(SplitFilePlan {
        from_file: request.from_file,
        to_file: request.to_file,
        from_dialect: request.from_dialect,
        to_dialect: request.to_dialect,
        items,
        from_rewritten,
        to_rewritten,
        to_file_existed: request.to_file_existed,
        to_parent_existed: request.to_parent_existed,
        changed,
        written,
    })
}

fn top_level_path_index(path: &Path, command: &str) -> Result<usize> {
    match path.indexes() {
        [index] => Ok(index.get()),
        _ => anyhow::bail!("{command} requires a top-level path, for example --path 2"),
    }
}
