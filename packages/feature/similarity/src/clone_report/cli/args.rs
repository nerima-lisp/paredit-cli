use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};

use crate::clone_report::domain::{
    ClassOverlapPolicy, DEFAULT_BUCKET_WIDTH, DEFAULT_CALIBRATION_FLOOR,
    DEFAULT_HELPER_OVERHEAD_LINES, DEFAULT_MIN_GAP_BUCKETS, DEFAULT_MIN_SAMPLE, SequenceMatchMode,
    SequenceOverlapPolicy,
};
use crate::similarity_report::cli::types::ErrorPolicy;
use crate::similarity_report::usecase::{SimilarityComparisonScope, SimilarityFormScope};

/// Where to look and what to do about files that will not read.
///
/// Flattened into all five commands so `clone-classes` and `clone-sequences`
/// discover the same tree from the same flags, and so a caller that has tuned
/// `--exclude` for one can reuse it verbatim for the others.
#[derive(Debug, Args)]
pub struct CloneDiscoveryArgs {
    /// Files or directories to scan recursively.
    #[arg(required = true)]
    pub roots: Vec<PathBuf>,
    /// Include files whose extension does not identify a known Lisp dialect.
    #[arg(long)]
    pub include_unknown: bool,
    /// Include hidden directories and files.
    #[arg(long)]
    pub include_hidden: bool,
    /// Include generated or dependency directories such as target and node_modules.
    #[arg(long)]
    pub include_generated: bool,
    /// Maximum directory recursion depth from each root directory.
    #[arg(long)]
    pub max_depth: Option<usize>,
    /// Exclude an exact file or directory subtree from discovery. May be repeated.
    #[arg(long)]
    pub exclude: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Control whether a file processing error stops the report or skips that file.
    #[arg(long, default_value = "fail")]
    pub error_policy: ErrorPolicy,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

/// What counts as a candidate form and how close two of them have to be.
#[derive(Debug, Args)]
pub struct CloneMatchArgs {
    /// Minimum normalized similarity for two forms to join the same class.
    #[arg(long, default_value_t = 0.87)]
    pub threshold: f64,
    /// Minimum expression node count for a candidate form.
    #[arg(long, default_value_t = 4)]
    pub min_node_count: usize,
    /// Minimum number of source lines spanned by a candidate form.
    #[arg(long, default_value_t = 1)]
    pub min_line_span: usize,
    /// Restrict comparisons based on whether forms belong to the same file.
    #[arg(long, default_value = "all")]
    pub comparison_scope: SimilarityComparisonScope,
    /// Restrict candidates to all forms or only top-level forms.
    #[arg(long, default_value = "all")]
    pub form_scope: SimilarityFormScope,
    /// Control whether a group wholly nested inside a higher-ranked one is reported.
    #[arg(long, default_value = "maximal")]
    pub overlap_policy: ClassOverlapPolicy,
    /// Maximum number of tree-edit-distance comparisons to evaluate.
    #[arg(long)]
    pub max_comparisons: Option<usize>,
    /// Maximum number of candidate forms to retain across all scanned files.
    #[arg(long)]
    pub max_candidates: Option<usize>,
    /// Maximum number of ranked pairs to retain before grouping.
    #[arg(long)]
    pub max_results: Option<usize>,
}

/// Ranking knobs shared by the reports that estimate an extraction.
#[derive(Debug, Args)]
pub struct CloneExtractionArgs {
    /// Lines a extracted helper costs beyond its body, used by the savings estimate.
    #[arg(long, default_value_t = DEFAULT_HELPER_OVERHEAD_LINES)]
    pub helper_overhead_lines: usize,
}

#[derive(Debug, Args)]
pub struct CloneClassReportArgs {
    #[command(flatten)]
    pub discovery: CloneDiscoveryArgs,
    #[command(flatten)]
    pub matching: CloneMatchArgs,
    #[command(flatten)]
    pub extraction: CloneExtractionArgs,
    /// Minimum number of forms a class must contain to be reported.
    #[arg(long, default_value_t = 2)]
    pub min_members: usize,
    /// Report only classes of this clone type (1, 2 or 3).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
    pub clone_type: Option<u8>,
    /// Maximum number of ranked classes to print.
    #[arg(long)]
    pub max_classes: Option<usize>,
    /// Exit unsuccessfully after printing when any class is reported.
    #[arg(long)]
    pub fail_on_clones: bool,
}

#[derive(Debug, Args)]
pub struct CloneSequenceReportArgs {
    #[command(flatten)]
    pub discovery: CloneDiscoveryArgs,
    #[command(flatten)]
    pub extraction: CloneExtractionArgs,
    /// Minimum number of adjacent sibling forms in a reported run.
    #[arg(long, default_value_t = 3)]
    pub min_run_length: usize,
    /// Maximum run length to enumerate. Longer runs are whole bodies, which the
    /// form-shaped reports already cover.
    #[arg(long, default_value_t = 16)]
    pub max_run_length: usize,
    /// Minimum number of non-overlapping occurrences for a reported group.
    #[arg(long, default_value_t = 2)]
    pub min_occurrences: usize,
    /// Minimum total expression node count for a reported run.
    #[arg(long, default_value_t = 8)]
    pub min_run_nodes: usize,
    /// Whether runs must be identical or may differ in identifiers only.
    #[arg(long, default_value = "renamed")]
    pub match_mode: SequenceMatchMode,
    /// Control whether runs contained by longer reported runs are reported.
    #[arg(long, default_value = "maximal")]
    pub overlap_policy: SequenceOverlapPolicy,
    /// Maximum number of ranked groups to print.
    #[arg(long)]
    pub max_groups: Option<usize>,
    /// Also report runs whose enclosing forms are themselves clones, which inspect clone-classes already reports.
    #[arg(long)]
    pub include_parent_clones: bool,
    /// Exit unsuccessfully after printing when any group is reported.
    #[arg(long)]
    pub fail_on_clones: bool,
}

#[derive(Debug, Args)]
pub struct CloneExternalReportArgs {
    #[command(flatten)]
    pub discovery: CloneDiscoveryArgs,
    #[command(flatten)]
    pub matching: CloneMatchArgs,
    /// Reference corpus to compare against: a dependency checkout, a vendored
    /// library, or any tree whose code this project should not be reinventing.
    /// May be repeated. Required.
    #[arg(long, required = true)]
    pub reference: Vec<PathBuf>,
    /// Apply the generated-directory skip to the reference corpus too.
    ///
    /// Off by default, unlike every other scan in this tool: a reference corpus
    /// is nearly always a `vendor/`, `target/` or `node_modules/` tree, and
    /// skipping those would leave nothing to compare against. The roots were
    /// named explicitly, so there is nothing to protect the caller from.
    #[arg(long)]
    pub reference_skip_generated: bool,
    /// Exit unsuccessfully after printing when any external match is reported.
    #[arg(long)]
    pub fail_on_matches: bool,
}

#[derive(Debug, Args)]
pub struct CloneThresholdReportArgs {
    #[command(flatten)]
    pub discovery: CloneDiscoveryArgs,
    #[command(flatten)]
    pub matching: CloneMatchArgs,
    /// Lowest similarity to include in the sampled distribution.
    #[arg(long, default_value_t = DEFAULT_CALIBRATION_FLOOR)]
    pub floor: f64,
    /// Histogram bucket width.
    #[arg(long, default_value_t = DEFAULT_BUCKET_WIDTH)]
    pub bucket_width: f64,
    /// Scored pairs required before a recommendation is called well supported.
    #[arg(long, default_value_t = DEFAULT_MIN_SAMPLE)]
    pub min_sample: usize,
    /// Empty histogram buckets required before a gap outranks the variance split.
    #[arg(long, default_value_t = DEFAULT_MIN_GAP_BUCKETS)]
    pub min_gap_buckets: usize,
}

#[derive(Debug, Args)]
pub struct CloneGenealogyReportArgs {
    #[command(flatten)]
    pub discovery: CloneDiscoveryArgs,
    #[command(flatten)]
    pub matching: CloneMatchArgs,
    #[command(flatten)]
    pub extraction: CloneExtractionArgs,
    /// Minimum number of forms a class must contain to be reported.
    #[arg(long, default_value_t = 2)]
    pub min_members: usize,
    /// Maximum number of ranked classes to trace.
    #[arg(long)]
    pub max_classes: Option<usize>,
    /// Exit unsuccessfully after printing when git cannot date a clone member.
    #[arg(long)]
    pub fail_on_undated: bool,
}
