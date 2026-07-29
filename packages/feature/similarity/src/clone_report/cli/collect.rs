//! Turning roots into candidate forms, once, for all five commands.
//!
//! Two shapes, because the reports want different things. Four of them want
//! *candidates* and never look at a tree again, so the tree is dropped as soon
//! as its candidates are out — that is what keeps a workspace-wide run's peak
//! memory proportional to the candidates rather than to the sources.
//! `clone-sequences` verifies its groups against the tree afterwards and so has
//! to keep them, which is the memory it costs and the reason it is not the
//! default shape.

use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_core_workspace::workspace::WorkspaceDiscovery;

use crate::similarity_report::cli::types::ErrorPolicy;
use crate::similarity_report::domain::{
    SimilarityCandidate, SimilarityReportOptions, collect_similarity_candidates,
};

use super::args::CloneDiscoveryArgs;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoverySummary {
    pub scanned_files: usize,
    pub skipped_unknown: usize,
    pub skipped_hidden: usize,
    pub skipped_generated: usize,
    pub skipped_symlink: usize,
    pub skipped_excluded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusFileError {
    pub path: PathBuf,
    pub stage: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub struct CandidateCorpus {
    pub candidates: Vec<SimilarityCandidate>,
    pub omitted_candidates: usize,
    pub summary: DiscoverySummary,
    pub errors: Vec<CorpusFileError>,
}

#[derive(Debug)]
pub struct ParsedSource {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub text: String,
    pub tree: SyntaxTree,
}

#[derive(Debug)]
pub struct SourceCorpus {
    pub sources: Vec<ParsedSource>,
    pub summary: DiscoverySummary,
    pub errors: Vec<CorpusFileError>,
}

/// Discovery for one root set, with `include_generated` overridable.
///
/// The reference corpus of `clone-external` is the one place where scanning
/// `vendor/` and `target/` is the point rather than a mistake.
struct Scan<'a> {
    roots: &'a [PathBuf],
    args: &'a CloneDiscoveryArgs,
    include_generated: bool,
}

pub fn collect_candidates(
    roots: &[PathBuf],
    args: &CloneDiscoveryArgs,
    options: &SimilarityReportOptions,
) -> CliResult<CandidateCorpus> {
    collect_candidates_with_generated(roots, args, options, args.input.include_generated)
}

pub fn collect_candidates_with_generated(
    roots: &[PathBuf],
    args: &CloneDiscoveryArgs,
    options: &SimilarityReportOptions,
    include_generated: bool,
) -> CliResult<CandidateCorpus> {
    let scan = Scan {
        roots,
        args,
        include_generated,
    };
    let (files, summary, discovery) = discover(&scan)?;

    let mut candidates = Vec::new();
    let mut omitted_candidates = 0usize;
    let mut errors = Vec::new();
    for path in files {
        let dialect = Dialect::detect(Some(&path), args.dialect.map(Into::into));
        let parsed = match read_and_parse(&discovery, &path, dialect) {
            Ok(parsed) => parsed,
            Err(error) => {
                push_error(&mut errors, error, args.error_policy)?;
                continue;
            }
        };
        match collect_similarity_candidates(
            &parsed.tree,
            &parsed.text,
            &parsed.path,
            dialect,
            options,
            &mut candidates,
        ) {
            Ok(omitted) => omitted_candidates = omitted_candidates.saturating_add(omitted),
            Err(error) => push_error(
                &mut errors,
                CorpusFileError {
                    path: parsed.path.clone(),
                    stage: "collect",
                    message: error.to_string(),
                },
                args.error_policy,
            )?,
        }
    }

    Ok(CandidateCorpus {
        candidates,
        omitted_candidates,
        summary,
        errors,
    })
}

pub fn collect_sources(roots: &[PathBuf], args: &CloneDiscoveryArgs) -> CliResult<SourceCorpus> {
    let scan = Scan {
        roots,
        args,
        include_generated: args.input.include_generated,
    };
    let (files, summary, discovery) = discover(&scan)?;

    let mut sources = Vec::new();
    let mut errors = Vec::new();
    for path in files {
        let dialect = Dialect::detect(Some(&path), args.dialect.map(Into::into));
        match read_and_parse(&discovery, &path, dialect) {
            Ok(parsed) => sources.push(parsed),
            Err(error) => push_error(&mut errors, error, args.error_policy)?,
        }
    }

    Ok(SourceCorpus {
        sources,
        summary,
        errors,
    })
}

fn discover(scan: &Scan<'_>) -> CliResult<(Vec<PathBuf>, DiscoverySummary, WorkspaceDiscovery)> {
    // Every input selector and filter comes from the shared flag block, so
    // `--since`, `--paths-from`, `--include` and the cache work here exactly as
    // they do for `inspect similarity`. Only the two overrides this slice
    // genuinely needs are applied on top.
    let resolved = scan.args.input.resolve(scan.roots)?;
    let mut options = resolved.options;
    options.include_unknown = scan.args.input.include_unknown || scan.args.dialect.is_some();
    options.include_generated = scan.include_generated;
    let (discovery, _cache_outcome) =
        scan.args
            .input
            .scan(&paredit_core_cli::workspace_args::ResolvedWorkspaceInput {
                options,
                ..resolved
            })?;
    let summary = DiscoverySummary {
        scanned_files: discovery.files().len(),
        skipped_unknown: discovery.skipped_unknown_count(),
        skipped_hidden: discovery.skipped_hidden_count(),
        skipped_generated: discovery.skipped_generated_count(),
        skipped_symlink: discovery.skipped_symlink_count(),
        skipped_excluded: discovery.skipped_excluded_count(),
    };
    let files = discovery.files().to_vec();
    Ok((files, summary, discovery))
}

fn read_and_parse(
    discovery: &paredit_core_workspace::workspace::WorkspaceDiscovery,
    path: &std::path::Path,
    dialect: Dialect,
) -> std::result::Result<ParsedSource, CorpusFileError> {
    let bytes = discovery.read_file(path).map_err(|error| CorpusFileError {
        path: path.to_path_buf(),
        stage: "read",
        message: error.root_cause().to_string(),
    })?;
    let text = String::from_utf8(bytes).map_err(|error| CorpusFileError {
        path: path.to_path_buf(),
        stage: "read",
        message: error.to_string(),
    })?;
    let tree = SyntaxTree::parse_with_dialect(&text, dialect).map_err(|error| CorpusFileError {
        path: path.to_path_buf(),
        stage: "parse",
        message: error.to_string(),
    })?;
    Ok(ParsedSource {
        path: path.to_path_buf(),
        dialect,
        text,
        tree,
    })
}

/// Under `--error-policy fail` the first bad file ends the run; under `skip` it
/// joins the report's error list and the scan continues.
fn push_error(
    errors: &mut Vec<CorpusFileError>,
    error: CorpusFileError,
    policy: ErrorPolicy,
) -> CliResult<()> {
    if policy == ErrorPolicy::Fail {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::EnvironmentIo,
            format!(
                "failed to {} {}: {}",
                error.stage,
                error.path.display(),
                error.message
            ),
        )
        .into());
    }
    errors.push(error);
    Ok(())
}
