use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::application::usecase::function_parameter::FunctionParameterSection;
use crate::domain::sexpr::{Path, SymbolName};
use crate::presentation::cli::args::{DialectArg, OutputFormat, ParameterInsert};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::presentation::cli::function_parameter) enum ParameterSection {
    Auto,
    Positional,
    Optional,
    Keyword,
}

impl ParameterSection {
    pub(in crate::presentation::cli::function_parameter) const fn into_function_parameter_section(
        self,
    ) -> FunctionParameterSection {
        match self {
            Self::Auto => FunctionParameterSection::Auto,
            Self::Positional => FunctionParameterSection::Positional,
            Self::Optional => FunctionParameterSection::Optional,
            Self::Keyword => FunctionParameterSection::Keyword,
        }
    }
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct AddFunctionParameterArgs {
    #[arg(short, long)]
    pub(in crate::presentation::cli::function_parameter) file: Option<PathBuf>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) dialect: Option<DialectArg>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) definition_path: Path,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) name: SymbolName,
    #[arg(long, allow_hyphen_values = true)]
    pub(in crate::presentation::cli::function_parameter) argument: String,
    #[arg(long = "call-path")]
    pub(in crate::presentation::cli::function_parameter) call_paths: Vec<Path>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) all_calls: bool,
    #[arg(long, value_enum, default_value_t = ParameterInsert::End)]
    pub(in crate::presentation::cli::function_parameter) insert: ParameterInsert,
    #[arg(
        long = "parameter-section",
        value_enum,
        default_value_t = ParameterSection::Auto,
        help = "Target lambda-list section: auto, positional, optional, or keyword"
    )]
    pub(in crate::presentation::cli::function_parameter) section: ParameterSection,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::function_parameter) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct MoveFunctionParameterArgs {
    #[arg(short, long)]
    pub(in crate::presentation::cli::function_parameter) file: Option<PathBuf>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) dialect: Option<DialectArg>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) definition_path: Path,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) name: SymbolName,
    #[arg(long = "to-index")]
    pub(in crate::presentation::cli::function_parameter) to_index: usize,
    #[arg(long = "call-path")]
    pub(in crate::presentation::cli::function_parameter) call_paths: Vec<Path>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) all_calls: bool,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::function_parameter) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SwapFunctionParametersArgs {
    #[arg(short, long)]
    pub(in crate::presentation::cli::function_parameter) file: Option<PathBuf>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) dialect: Option<DialectArg>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) definition_path: Path,
    #[arg(long = "left-name")]
    pub(in crate::presentation::cli::function_parameter) left_name: SymbolName,
    #[arg(long = "right-name")]
    pub(in crate::presentation::cli::function_parameter) right_name: SymbolName,
    #[arg(long = "call-path")]
    pub(in crate::presentation::cli::function_parameter) call_paths: Vec<Path>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) all_calls: bool,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::function_parameter) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct ReorderFunctionParametersArgs {
    #[arg(short, long)]
    pub(in crate::presentation::cli::function_parameter) file: Option<PathBuf>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) dialect: Option<DialectArg>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) definition_path: Path,
    #[arg(long = "parameter", required = true)]
    pub(in crate::presentation::cli::function_parameter) parameter_order: Vec<SymbolName>,
    #[arg(long = "call-path")]
    pub(in crate::presentation::cli::function_parameter) call_paths: Vec<Path>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) all_calls: bool,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::function_parameter) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RemoveFunctionParameterArgs {
    #[arg(short, long)]
    pub(in crate::presentation::cli::function_parameter) file: Option<PathBuf>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) dialect: Option<DialectArg>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) definition_path: Path,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) name: SymbolName,
    #[arg(long = "call-path")]
    pub(in crate::presentation::cli::function_parameter) call_paths: Vec<Path>,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) all_calls: bool,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) allow_missing_argument: bool,
    #[arg(long)]
    pub(in crate::presentation::cli::function_parameter) write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::function_parameter) output: OutputFormat,
}
