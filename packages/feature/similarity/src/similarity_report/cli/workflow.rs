use anyhow::{Result, bail};

use crate::application::usecase::similarity_report::{
    DiscoveredSimilarityFile, SimilarityDuplicatePolicy, SimilarityGateDecision,
    SimilarityIndeterminateReason, SimilarityInventory, SimilarityReportOptions,
    SimilarityReportRequest, SimilarityReportSourcePort, build_similarity_report,
};
use crate::domain::dialect::Dialect;
use crate::infrastructure::workspace::{
    WorkspaceDiscovery, WorkspaceDiscoveryOptions, discover_workspace_files,
};

use super::args::SimilarityReportArgs;
use super::render::print_similarity_report;

pub fn similarity_report(args: SimilarityReportArgs) -> Result<()> {
    let options = SimilarityReportOptions::new(
        args.threshold,
        args.min_node_count,
        args.min_line_span,
        args.comparison_scope,
        args.form_scope,
        args.overlap_policy,
        args.max_candidates,
        args.max_comparisons,
        args.max_results,
    )?;
    let request = SimilarityReportRequest {
        roots: args.roots.clone(),
        include_unknown: args.include_unknown,
        include_hidden: args.include_hidden,
        include_generated: args.include_generated,
        max_depth: args.max_depth,
        exclude: args.exclude.clone(),
        forced_dialect: args.dialect.map(Into::into),
        options,
        error_policy: args.error_policy.into(),
        duplicate_policy: if args.fail_on_duplicates {
            SimilarityDuplicatePolicy::Fail
        } else {
            SimilarityDuplicatePolicy::Ignore
        },
    };

    let mut source = CliSimilarityReportSource::default();
    let plan = build_similarity_report(&mut source, request)?;
    print_similarity_report(&plan, &args)?;

    match plan.gate() {
        SimilarityGateDecision::NotRequested | SimilarityGateDecision::Passed => Ok(()),
        SimilarityGateDecision::DuplicateFound { matched_pairs } => {
            Err(crate::presentation::cli::gate::gate_failure(format!(
                "similarity-report policy failed: {matched_pairs} duplicate pair(s) found"
            )))
        }
        SimilarityGateDecision::Indeterminate(SimilarityIndeterminateReason::ComparisonLimit {
            unprocessed_pairs,
        }) => bail!(
            "similarity-report policy indeterminate: comparison limit reached with {unprocessed_pairs} pair(s) unprocessed"
        ),
        SimilarityGateDecision::Indeterminate(SimilarityIndeterminateReason::CandidateLimit {
            omitted_candidates,
        }) => bail!(
            "similarity-report policy indeterminate: candidate limit reached with {omitted_candidates} candidate(s) omitted"
        ),
        SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::ProcessingErrors { file_count },
        ) => bail!(
            "similarity-report policy indeterminate: {file_count} file(s) skipped due to processing errors"
        ),
    }
}

#[derive(Default)]
struct CliSimilarityReportSource {
    discovery: Option<WorkspaceDiscovery>,
}

impl SimilarityReportSourcePort for CliSimilarityReportSource {
    fn discover(
        &mut self,
        request: &SimilarityReportRequest,
    ) -> anyhow::Result<SimilarityInventory> {
        let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
            roots: request.roots.clone(),
            include_unknown: request.include_unknown || request.forced_dialect.is_some(),
            include_hidden: request.include_hidden,
            include_generated: request.include_generated,
            max_depth: request.max_depth,
            exclude: request.exclude.clone(),
        })?;
        let inventory = SimilarityInventory {
            files: discovery
                .files()
                .iter()
                .map(|path| DiscoveredSimilarityFile {
                    path: path.clone(),
                    dialect: Dialect::detect(Some(path), request.forced_dialect),
                })
                .collect(),
            skipped_unknown_count: discovery.skipped_unknown_count(),
            skipped_hidden_count: discovery.skipped_hidden_count(),
            skipped_generated_count: discovery.skipped_generated_count(),
            skipped_symlink_count: discovery.skipped_symlink_count(),
            skipped_excluded_count: discovery.skipped_excluded_count(),
        };
        self.discovery = Some(discovery);
        Ok(inventory)
    }

    fn load(&self, file: &DiscoveredSimilarityFile) -> Result<Vec<u8>, String> {
        let discovery = self
            .discovery
            .as_ref()
            .ok_or_else(|| "similarity source was loaded before discovery".to_owned())?;
        discovery
            .read_file(&file.path)
            .map_err(|error| error.root_cause().to_string())
    }
}
