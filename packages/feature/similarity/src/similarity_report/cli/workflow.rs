use paredit_core_cli::CommandResult;

use crate::similarity_report::usecase::{
    DiscoveredSimilarityFile, SimilarityDuplicatePolicy, SimilarityGateDecision,
    SimilarityIndeterminateReason, SimilarityInventory, SimilarityReportOptions,
    SimilarityReportPlan, SimilarityReportRequest, SimilarityReportSourcePort,
    build_similarity_report,
};
use paredit_core_cli::workspace_args::scan_workspace;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_workspace::workspace::{
    DiscoveryCache, WorkspaceDiscovery, WorkspaceDiscoveryOptions,
};

use super::args::SimilarityReportArgs;
use super::cache::{ResultCacheOutcome, SimilarityResultCache, fingerprint_corpus};
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
        inventory: None,
    };

    let (plan, cache_outcome) = match args.input.cache_dir.as_deref() {
        None => (build_similarity_report(&mut source, request)?, None),
        Some(directory) => {
            let cache = SimilarityResultCache::open(directory, args.input.clear_cache)?;
            reuse_or_build(&mut source, request, &cache)?
        }
    };
    print_similarity_report(&plan, &args, cache_outcome)?;

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

/// Answers from the stored report when the corpus and the options match.
///
/// Discovery has to run before that can be decided, because the corpus is part
/// of the key — see [`super::cache`]. The source memoises it, so the analysis
/// on a miss does not walk the tree a second time.
fn reuse_or_build(
    source: &mut CliSimilarityReportSource,
    request: SimilarityReportRequest,
    cache: &SimilarityResultCache,
) -> paredit_core_cli::CliResult<(SimilarityReportPlan, Option<ResultCacheOutcome>)> {
    let inventory = source.discover(&request)?;
    let Some(fingerprint) = fingerprint_corpus(&inventory, |file| source.load(file)) else {
        // A file the corpus cannot read at all. Reported by the analysis under
        // the error policy, but not cached: see `fingerprint_corpus`.
        return Ok((build_similarity_report(source, request)?, None));
    };

    let key = cache.key(&request, &fingerprint);
    if let Some(plan) = cache.get(&key, &inventory, request.duplicate_policy) {
        return Ok((plan, Some(ResultCacheOutcome::Hit)));
    }
    let plan = build_similarity_report(source, request)?;
    cache.put(&key, &plan);
    Ok((plan, Some(ResultCacheOutcome::Missing)))
}

struct CliSimilarityReportSource {
    /// Held rather than taken from the args: `--dialect` widens the options
    /// below, and a cache key computed from the un-widened set would serve a
    /// narrower file list than the run asked for.
    cache: Option<DiscoveryCache>,
    options: WorkspaceDiscoveryOptions,
    from_list: bool,
    discovery: Option<WorkspaceDiscovery>,
    /// The inventory the first `discover` produced.
    ///
    /// `reuse_or_build` needs it before it can decide whether to run the
    /// analysis, and the analysis asks for it again. Recomputing would mean
    /// walking the tree twice per run, and — worse — deciding the cache key
    /// against one walk while analysing the results of another.
    inventory: Option<SimilarityInventory>,
}

impl SimilarityReportSourcePort for CliSimilarityReportSource {
    type Error = paredit_core_cli::CliError;

    fn discover(
        &mut self,
        request: &SimilarityReportRequest,
    ) -> Result<SimilarityInventory, Self::Error> {
        if let Some(inventory) = self.inventory.clone() {
            return Ok(inventory);
        }
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
        self.inventory = Some(inventory.clone());
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
