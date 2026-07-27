use anyhow::{Context, Result, anyhow};

use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::definition_reference::{
    collect_package_form_spans, collect_reference_needles, collect_symbol_references,
};
use crate::domain::dialect::Dialect;
use crate::domain::remove_unused_definition::types::{
    RemoveUnusedDefinitionInputFile, UnusedDefinitionDefinition,
};
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName, SyntaxTree};

#[derive(Debug)]
pub(super) struct UnusedDefinitionItem {
    pub(super) definition: UnusedDefinitionDefinition,
    pub(super) references: Vec<DefinitionReference>,
}

#[derive(Debug)]
pub(super) struct UnusedDefinitionFile {
    pub(super) definitions: Vec<UnusedDefinitionItem>,
}

#[derive(Debug)]
pub(super) struct DefinitionReference;

pub(super) fn collect_unused_definition_candidates(
    files: &[RemoveUnusedDefinitionInputFile],
) -> Result<Vec<UnusedDefinitionFile>> {
    for file in files {
        if file.dialect == Dialect::Unknown {
            anyhow::bail!(
                "remove-unused-definition does not support dialect unknown: {}",
                file.path.display()
            );
        }
    }

    let parsed_files = files
        .iter()
        .map(|file| -> Result<_> {
            let tree = SyntaxTree::parse_with_dialect(&file.text, file.dialect)
                .with_context(|| format!("failed to parse {}", file.path.display()))?;
            Ok((file, tree.root_view()))
        })
        .collect::<Result<Vec<_>>>()?;

    let package_form_spans: Vec<Vec<ByteSpan>> = parsed_files
        .iter()
        .map(|(file, view)| {
            let mut spans = Vec::new();
            collect_package_form_spans(file.dialect, view, &mut spans);
            spans
        })
        .collect();
    let atom_needles: Vec<std::collections::HashSet<String>> = parsed_files
        .iter()
        .map(|(_, view)| {
            let mut needles = std::collections::HashSet::new();
            collect_reference_needles(view, &mut needles);
            needles
        })
        .collect();

    let worker_count = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .clamp(1, files.len().max(1));
    let mut ordered: Vec<Option<Result<UnusedDefinitionFile>>> =
        (0..files.len()).map(|_| None).collect();
    std::thread::scope(|scope| -> Result<()> {
        let parsed_files = &parsed_files;
        let package_form_spans = &package_form_spans;
        let atom_needles = &atom_needles;
        let handles: Vec<_> = (0..worker_count)
            .map(|worker| {
                scope.spawn(move || {
                    files
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(worker_count)
                        .map(|(file_index, file)| {
                            (
                                file_index,
                                file_unused_definition_candidates(
                                    files,
                                    parsed_files,
                                    package_form_spans,
                                    atom_needles,
                                    file_index,
                                    file,
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            for (file_index, report) in handle
                .join()
                .map_err(|_| anyhow!("unused-definition candidate worker thread panicked"))?
            {
                ordered[file_index] = Some(report);
            }
        }
        Ok(())
    })?;
    ordered.into_iter().flatten().collect()
}

fn file_unused_definition_candidates(
    files: &[RemoveUnusedDefinitionInputFile],
    parsed_files: &[(&RemoveUnusedDefinitionInputFile, ExpressionView)],
    package_form_spans: &[Vec<ByteSpan>],
    atom_needles: &[std::collections::HashSet<String>],
    file_index: usize,
    file: &RemoveUnusedDefinitionInputFile,
) -> Result<UnusedDefinitionFile> {
    let named_definitions = file
        .definitions
        .iter()
        .filter_map(|definition| {
            let name = definition.name.as_ref()?;
            Some((definition, name))
        })
        .filter_map(|(definition, name)| match SymbolName::new(name.clone()) {
            Ok(symbol) => Some(Ok((definition, symbol))),
            Err(_) if !definition.category.is_bulk_removable() => None,
            Err(error) => Some(Err(error.context(format!(
                "remove-unused-definition found invalid symbol '{name}' in {}",
                file.path.display()
            )))),
        })
        .collect::<Result<Vec<_>>>()?;

    let definitions = named_definitions
        .into_iter()
        .map(|(definition, symbol)| {
            let needle = common_lisp_symbol_reference_needle(symbol.as_str());
            let references = files
                .iter()
                .enumerate()
                .flat_map(|(other_index, other)| {
                    let (_, other_view) = &parsed_files[other_index];
                    let mut spans = Vec::new();
                    if atom_needles[other_index].contains(&needle) {
                        collect_symbol_references(
                            other.dialect,
                            other_view,
                            &symbol,
                            &other.text,
                            &mut spans,
                        );
                        let package_spans = &package_form_spans[other_index];
                        spans.retain(|span| {
                            !package_spans
                                .iter()
                                .any(|package| package.contains_span(*span))
                        });
                    }
                    spans
                        .into_iter()
                        .filter(move |span| {
                            !(other_index == file_index && definition.span.contains_span(*span))
                        })
                        .map(|_span| DefinitionReference)
                })
                .collect();

            UnusedDefinitionItem {
                definition: definition.clone(),
                references,
            }
        })
        .collect();

    Ok(UnusedDefinitionFile { definitions })
}
