use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::function_parameter::usecase::FunctionParameterSection;
use paredit_core_cli::args::{DialectArg, OutputFormat, ParameterInsert};
use paredit_core_syntax::sexpr::{Path, SymbolName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ParameterSection {
    Auto,
    Positional,
    Optional,
    Keyword,
}

impl ParameterSection {
    #[must_use]
    pub const fn into_function_parameter_section(self) -> FunctionParameterSection {
        match self {
            Self::Auto => FunctionParameterSection::Auto,
            Self::Positional => FunctionParameterSection::Positional,
            Self::Optional => FunctionParameterSection::Optional,
            Self::Keyword => FunctionParameterSection::Keyword,
        }
    }
}

#[derive(Debug, Args)]
pub struct AddFunctionParameterArgs {
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[arg(long)]
    pub definition_path: Path,
    #[arg(long)]
    pub name: SymbolName,
    #[arg(long, allow_hyphen_values = true)]
    pub argument: String,
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    #[arg(long)]
    pub all_calls: bool,
    #[arg(long, value_enum, default_value_t = ParameterInsert::End)]
    pub insert: ParameterInsert,
    #[arg(
        long = "parameter-section",
        value_enum,
        default_value_t = ParameterSection::Auto,
        help = "Target lambda-list section: auto, positional, optional, or keyword"
    )]
    pub section: ParameterSection,
    #[arg(long)]
    pub write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct MoveFunctionParameterArgs {
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[arg(long)]
    pub definition_path: Path,
    #[arg(long)]
    pub name: SymbolName,
    #[arg(long = "to-index")]
    pub to_index: usize,
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    #[arg(long)]
    pub all_calls: bool,
    #[arg(long)]
    pub write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct SwapFunctionParametersArgs {
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[arg(long)]
    pub definition_path: Path,
    #[arg(long = "left-name")]
    pub left_name: SymbolName,
    #[arg(long = "right-name")]
    pub right_name: SymbolName,
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    #[arg(long)]
    pub all_calls: bool,
    #[arg(long)]
    pub write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ReorderFunctionParametersArgs {
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[arg(long)]
    pub definition_path: Path,
    #[arg(long = "parameter", required = true)]
    pub parameter_order: Vec<SymbolName>,
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    #[arg(long)]
    pub all_calls: bool,
    #[arg(long)]
    pub write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RemoveFunctionParameterArgs {
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[arg(long)]
    pub definition_path: Path,
    #[arg(long)]
    pub name: SymbolName,
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    #[arg(long)]
    pub all_calls: bool,
    #[arg(long)]
    pub allow_missing_argument: bool,
    #[arg(long)]
    pub write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
