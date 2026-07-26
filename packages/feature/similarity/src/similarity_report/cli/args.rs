use std::path::PathBuf;

use clap::Args;

use crate::similarity_report::usecase::{
    SimilarityComparisonScope, SimilarityFormScope, SimilarityOverlapPolicy,
};

use paredit_core_cli::args::{DialectArg, OutputFormat};
use super::types::ErrorPolicy;

#[derive(Debug, Args)]
pub struct SimilarityReportArgs {
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
    /// Minimum normalized similarity to report.
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
    /// Control whether nested matches contained by higher-ranked matches are reported.
    #[arg(long, default_value = "maximal")]
    pub overlap_policy: SimilarityOverlapPolicy,
    /// Maximum number of tree-edit-distance comparisons to evaluate.
    #[arg(long)]
    pub max_comparisons: Option<usize>,
    /// Maximum number of candidate forms to retain across all scanned files.
    #[arg(long)]
    pub max_candidates: Option<usize>,
    /// Control whether a file processing error stops the report or skips that file.
    #[arg(long, default_value = "fail")]
    pub error_policy: ErrorPolicy,
    /// Maximum number of ranked pairs to include in the report.
    #[arg(long)]
    pub max_results: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
    /// Exit unsuccessfully after printing when similar pairs are found.
    #[arg(long)]
    pub fail_on_duplicates: bool,
}
