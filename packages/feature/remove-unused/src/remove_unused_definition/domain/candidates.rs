use crate::error::{AnalysisWorkerError, RemoveUnusedError, RemoveUnusedResult};

use crate::remove_unused_definition::domain::types::{
    RemoveUnusedDefinitionInputFile, UnusedDefinitionDefinition,
};
use paredit_core_semantics::definition_reference::{
    collect_package_form_spans, collect_reference_needles, collect_symbol_references,
};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SymbolName, SyntaxTree};

#[derive(Debug)]
pub struct UnusedDefinitionItem {
    pub definition: UnusedDefinitionDefinition,
    pub references: Vec<DefinitionReference>,
}

#[derive(Debug)]
pub struct UnusedDefinitionFile {
    pub definitions: Vec<UnusedDefinitionItem>,
}

#[derive(Debug)]
pub struct DefinitionReference;

pub fn collect_unused_definition_candidates(
    files: &[RemoveUnusedDefinitionInputFile],
) -> RemoveUnusedResult<Vec<UnusedDefinitionFile>> {
    for file in files {
        if file.dialect == Dialect::Unknown {
            return Err(RemoveUnusedError::UnsupportedDialect {
                operation: "remove-unused-definition",
                dialect: format!("unknown: {}", file.path.display()),
            });
        }
    }

    let parsed_files = files
        .iter()
        .map(|file| -> RemoveUnusedResult<_> {
            let tree =
                SyntaxTree::parse_with_dialect(&file.text, file.dialect).map_err(|source| {
                    RemoveUnusedError::ParseFailed {
                        path: file.path.display().to_string(),
                        source,
                    }
                })?;
            Ok((file, tree.root_view()))
        })
        .collect::<RemoveUnusedResult<Vec<_>>>()?;

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

    // `--jobs` governs this fan-out; `0` is that flag's own "as many as the
    // machine reports", and the only case that still asks the machine.
    let requested = paredit_core_safety::limits::effective_jobs();
    let worker_count = if requested == 0 {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    } else {
        requested
    }
    .clamp(1, files.len().max(1));
    let mut ordered: Vec<Option<RemoveUnusedResult<UnusedDefinitionFile>>> =
        (0..files.len()).map(|_| None).collect();
    std::thread::scope(|scope| -> RemoveUnusedResult<()> {
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
                .map_err(|_| AnalysisWorkerError::CandidateWorkerPanicked)?
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
) -> RemoveUnusedResult<UnusedDefinitionFile> {
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
            Err(source) => Some(Err(RemoveUnusedError::InvalidSymbol {
                operation: "remove-unused-definition",
                name: name.clone(),
                path: file.path.display().to_string(),
                source,
            })),
        })
        .collect::<RemoveUnusedResult<Vec<_>>>()?;

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
