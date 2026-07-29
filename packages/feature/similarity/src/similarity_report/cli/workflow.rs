use paredit_core_cli::CommandResult;

use crate::similarity_report::usecase::{
    DiscoveredSimilarityFile, SimilarityDuplicatePolicy, SimilarityGateDecision,
    SimilarityIndeterminateReason, SimilarityInventory, SimilarityReportOptions,
    SimilarityReportRequest, SimilarityReportSourcePort, build_similarity_report,
};
use paredit_core_cli::workspace_args::scan_workspace;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_workspace::workspace::{
    DiscoveryCache, WorkspaceDiscovery, WorkspaceDiscoveryOptions,
};

use super::args::SimilarityReportArgs;
use super::render::print_similarity_report;

pub fn similarity_report(args: SimilarityReportArgs) -> CommandResult {
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
    let resolved = args.input.resolve(&args.roots)?;
    let request = SimilarityReportRequest {
        roots: args.roots.clone(),
        include_unknown: args.input.include_unknown,
        include_hidden: args.input.include_hidden,
        include_generated: args.input.include_generated,
        max_depth: args.input.max_depth,
        exclude: args.input.exclude.clone(),
        forced_dialect: args.dialect.map(Into::into),
        options,
        error_policy: args.error_policy.into(),
        duplicate_policy: if args.fail_on_duplicates {
            SimilarityDuplicatePolicy::Fail
        } else {
            SimilarityDuplicatePolicy::Ignore
        },
    };

    let mut source = CliSimilarityReportSource {
        cache: args.input.cache()?,
        options: resolved.options,
        from_list: resolved.from_list,
        discovery: None,
    };
    let plan = build_similarity_report(&mut source, request)?;
    print_similarity_report(&plan, &args)?;

    match plan.gate() {
        SimilarityGateDecision::NotRequested | SimilarityGateDecision::Passed => Ok(()),
        SimilarityGateDecision::DuplicateFound { matched_pairs } => {
            Err(paredit_core_cli::gate::gate_failure(format!(
                "similarity-report policy failed: {matched_pairs} duplicate pair(s) found"
            )))
        }
        SimilarityGateDecision::Indeterminate(SimilarityIndeterminateReason::ComparisonLimit {
            unprocessed_pairs,
        }) => Err(paredit_core_cli::gate::gate_failure(format!(
            "similarity-report policy indeterminate: comparison limit reached with {unprocessed_pairs} pair(s) unprocessed"
        ))),
        SimilarityGateDecision::Indeterminate(SimilarityIndeterminateReason::CandidateLimit {
            omitted_candidates,
        }) => Err(paredit_core_cli::gate::gate_failure(format!(
            "similarity-report policy indeterminate: candidate limit reached with {omitted_candidates} candidate(s) omitted"
        ))),
        SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::ProcessingErrors { file_count },
        ) => Err(paredit_core_cli::gate::gate_failure(format!(
            "similarity-report policy indeterminate: {file_count} file(s) skipped due to processing errors"
        ))),
    }
}

struct CliSimilarityReportSource {
    /// Held rather than taken from the args: `--dialect` widens the options
    /// below, and a cache key computed from the un-widened set would serve a
    /// narrower file list than the run asked for.
    cache: Option<DiscoveryCache>,
    options: WorkspaceDiscoveryOptions,
    from_list: bool,
    discovery: Option<WorkspaceDiscovery>,
}

impl SimilarityReportSourcePort for CliSimilarityReportSource {
    type Error = paredit_core_cli::CliError;

    fn discover(
        &mut self,
        request: &SimilarityReportRequest,
    ) -> Result<SimilarityInventory, Self::Error> {
        // `--dialect` forces every file to be parsed with one dialect, so an
        // unknown extension is no longer a reason to skip a file.
        let options = WorkspaceDiscoveryOptions {
            include_unknown: self.options.include_unknown || request.forced_dialect.is_some(),
            ..self.options.clone()
        };
        let (discovery, _) = scan_workspace(&options, self.from_list, self.cache.as_ref())?;
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
