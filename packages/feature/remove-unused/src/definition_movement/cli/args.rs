use std::path::PathBuf;

use crate::application::usecase::sort_definitions::SortDefinitionsStrategy;
use crate::domain::definition::DefinitionCategory;
use crate::domain::sexpr::Path;
use clap::{Args, ValueEnum};

use super::super::{DialectArg, MoveInsert, OutputFormat};

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct MoveDefinitionArgs {
    /// Source file containing the top-level definition.
    #[arg(long)]
    pub(super) from_file: PathBuf,
    /// Destination file that will receive the definition. Missing files are planned as empty.
    #[arg(long)]
    pub(super) to_file: PathBuf,
    /// Override extension-based dialect detection for both files.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Top-level definition path from definition-report or outline, for example 2.
    #[arg(long)]
    pub(super) path: Path,
    /// Rewrite both files. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SplitFileArgs {
    /// Source file containing top-level definitions.
    #[arg(long)]
    pub(super) from_file: PathBuf,
    /// Destination file that will receive the definitions. Missing files are planned as empty.
    #[arg(long)]
    pub(super) to_file: PathBuf,
    /// Override extension-based dialect detection for both files.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Top-level definition paths from definition-report or outline, for example --path 2 --path 3.
    #[arg(long = "path")]
    pub(super) paths: Vec<Path>,
    /// Top-level definition names to move, for example --name render-widget --name with-rendering.
    #[arg(long = "name")]
    pub(super) names: Vec<String>,
    /// Top-level definition categories to move, for example --kind function --kind macro.
    #[arg(long = "kind", value_parser = parse_split_file_kind)]
    pub(super) categories: Vec<DefinitionCategory>,
    /// Rewrite both files and create the destination parent directory when needed.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SortDefinitionsArgs {
    /// File whose contiguous top-level definition blocks should be sorted.
    #[arg(short, long)]
    pub(super) file: PathBuf,
    /// Lisp dialect for definition classification.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Sorting strategy.
    #[arg(long, value_enum, default_value_t = SortDefinitionsOrderArg::Name)]
    pub(super) order: SortDefinitionsOrderArg,
    /// Rewrite the file instead of only printing the plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum SortDefinitionsOrderArg {
    Name,
    KindThenName,
}

impl From<SortDefinitionsOrderArg> for SortDefinitionsStrategy {
    fn from(value: SortDefinitionsOrderArg) -> Self {
        match value {
            SortDefinitionsOrderArg::Name => Self::Name,
            SortDefinitionsOrderArg::KindThenName => Self::KindThenName,
        }
    }
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct MoveFormArgs {
    /// Source file containing the top-level form.
    #[arg(long)]
    pub(super) from_file: PathBuf,
    /// Destination file that will receive the form. Missing files are planned as empty.
    #[arg(long)]
    pub(super) to_file: PathBuf,
    /// Override extension-based dialect detection for both files.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Top-level form path from outline, for example 2.
    #[arg(long)]
    pub(super) path: Path,
    /// Destination insertion strategy.
    #[arg(long, value_enum, default_value_t = MoveInsert::Append)]
    pub(super) insert: MoveInsert,
    /// Destination top-level anchor path. Required for --insert before/after.
    #[arg(long)]
    pub(super) anchor_path: Option<Path>,
    /// Rewrite both files. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct InsertTopLevelArgs {
    /// Source file receiving the top-level form.
    #[arg(short, long)]
    pub(super) file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Exactly one complete top-level S-expression to insert.
    #[arg(long)]
    pub(super) with: String,
    /// Insertion strategy.
    #[arg(long, value_enum, default_value_t = MoveInsert::Append)]
    pub(super) insert: MoveInsert,
    /// Destination top-level anchor path. Required for --insert before/after.
    #[arg(long)]
    pub(super) anchor_path: Option<Path>,
    /// Rewrite the file. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

fn parse_split_file_kind(value: &str) -> std::result::Result<DefinitionCategory, String> {
    DefinitionCategory::from_label(value).ok_or_else(|| {
        format!(
            "unknown split-file kind '{value}' (expected one of: function, macro, generic-function, method, class, struct, condition, variable, constant, parameter, package, system, test, customization, mode, other)"
        )
    })
}
