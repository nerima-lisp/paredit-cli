use std::path::Path;

use crate::domain::sexpr::SyntaxTree;
use crate::domain::similarity_report::{
    SimilarityCandidate, build_similarity_pairs_with_omissions, collect_similarity_candidates,
};

use super::types::{
    DiscoveredSimilarityFile, SimilarityErrorPolicy, SimilarityFileError, SimilarityInventory,
    SimilarityProcessingStage, SimilarityReportPlan, SimilarityReportRequest,
    SimilarityReportSourcePort, SimilarityReportWorkflowError,
};

struct FileProcessingOutput {
    candidates: Vec<SimilarityCandidate>,
    omitted_candidates: usize,
}

pub fn build_similarity_report(
    source: &mut impl SimilarityReportSourcePort,
    request: SimilarityReportRequest,
) -> Result<SimilarityReportPlan, SimilarityReportWorkflowError> {
    let inventory = source
        .discover(&request)
        .map_err(SimilarityReportWorkflowError::Source)?;
    let results = process_files(source, &inventory, &request)?;
    let mut candidates = Vec::new();
    let mut errors = Vec::new();
    let mut omitted_candidates = 0usize;

    for result in results {
        match result {
            Ok(output) => {
                candidates.extend(output.candidates);
                omitted_candidates = omitted_candidates.saturating_add(output.omitted_candidates);
            }
            Err(error) if request.error_policy == SimilarityErrorPolicy::Fail => {
                return Err(SimilarityReportWorkflowError::Processing(error));
            }
            Err(error) => errors.push(error),
        }
    }

    let report =
        build_similarity_pairs_with_omissions(candidates, omitted_candidates, &request.options)
            .map_err(SimilarityReportWorkflowError::Analysis)?;
    SimilarityReportPlan::new(report, inventory, errors, request.duplicate_policy)
        .map_err(SimilarityReportWorkflowError::InvalidPlan)
}

fn process_files(
    source: &impl SimilarityReportSourcePort,
    inventory: &SimilarityInventory,
    request: &SimilarityReportRequest,
) -> Result<Vec<Result<FileProcessingOutput, SimilarityFileError>>, SimilarityReportWorkflowError> {
    if inventory.files.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = source
        .available_parallelism()
        .get()
        .min(inventory.files.len());
    let chunk_size = inventory.files.len().div_ceil(worker_count);
    let mut chunks = std::thread::scope(|scope| {
        inventory
            .files
            .chunks(chunk_size)
            .map(|files| {
                scope.spawn(move || {
                    files
                        .iter()
                        .map(|file| process_file(source, file, &request.options))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join())
            .collect::<Vec<_>>()
    });

    let mut ordered = Vec::with_capacity(inventory.files.len());
    for chunk in chunks.drain(..) {
        ordered.extend(chunk.map_err(|_| SimilarityReportWorkflowError::WorkerPanicked)?);
    }
    Ok(ordered)
}

fn process_file(
    source: &impl SimilarityReportSourcePort,
    file: &DiscoveredSimilarityFile,
    options: &crate::domain::similarity_report::SimilarityReportOptions,
) -> Result<FileProcessingOutput, SimilarityFileError> {
    let bytes = source
        .load(file)
        .map_err(|message| file_error(&file.path, SimilarityProcessingStage::Read, message))?;
    let text = String::from_utf8(bytes).map_err(|error| {
        file_error(
            &file.path,
            SimilarityProcessingStage::Decode,
            error.to_string(),
        )
    })?;
    let tree = SyntaxTree::parse_with_dialect(&text, file.dialect).map_err(|error| {
        file_error(
            &file.path,
            SimilarityProcessingStage::Parse,
            error.to_string(),
        )
    })?;
    let mut candidates = Vec::new();
    let omitted_candidates = collect_similarity_candidates(
        &tree,
        &text,
        &file.path,
        file.dialect,
        options,
        &mut candidates,
    )
    .map_err(|error| {
        file_error(
            &file.path,
            SimilarityProcessingStage::Collect,
            error.to_string(),
        )
    })?;

    Ok(FileProcessingOutput {
        candidates,
        omitted_candidates,
    })
}

fn file_error(
    path: &Path,
    stage: SimilarityProcessingStage,
    message: String,
) -> SimilarityFileError {
    SimilarityFileError {
        path: path.to_path_buf(),
        stage,
        message,
    }
}
