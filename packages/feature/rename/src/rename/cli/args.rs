use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_syntax::sexpr::{Path, SymbolName};

#[derive(Debug, Args)]
pub struct RenameSymbolArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact source symbol atom.
    #[arg(long)]
    pub from: SymbolName,
    /// Exact replacement symbol atom.
    #[arg(long)]
    pub to: SymbolName,
    /// Print occurrence metadata instead of rewritten source.
    #[arg(long)]
    pub plan: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for --plan.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameInFormArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact source symbol atom.
    #[arg(long)]
    pub from: SymbolName,
    /// Exact replacement symbol atom.
    #[arg(long)]
    pub to: SymbolName,
    /// Select the refactor scope by child index path, for example 0.3.
    #[arg(long, conflicts_with = "at")]
    pub path: Option<Path>,
    /// Select the smallest refactor scope containing byte offset.
    #[arg(long, conflicts_with = "path")]
    pub at: Option<usize>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameBindingArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Select the let form by child index path, for example 0.3.
    #[arg(long, conflicts_with = "at")]
    pub path: Option<Path>,
    /// Select the smallest let form containing byte offset.
    #[arg(long, conflicts_with = "path")]
    pub at: Option<usize>,
    /// Existing binding name.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement binding name.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameSymbolsArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exact source symbol atom.
    #[arg(long)]
    pub from: SymbolName,
    /// Exact replacement symbol atom.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameFunctionArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing callable definition name.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement callable definition name.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameMacroletArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing macrolet or compiler-macrolet binding name.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement macrolet or compiler-macrolet binding name.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameSymbolMacroArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing Common Lisp define-symbol-macro binding name.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement Common Lisp define-symbol-macro binding name.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameLocalFunctionArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing Common Lisp local function binding name from flet or labels.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement Common Lisp local function binding name for flet or labels.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct WrapFunctionCallsArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing callable head to wrap.
    #[arg(long)]
    pub function: SymbolName,
    /// Wrapper callable or macro inserted around each selected call.
    #[arg(long)]
    pub wrapper: SymbolName,
    /// Wrapper form template containing exactly one "_" placeholder for the original call.
    #[arg(long = "wrapper-template")]
    pub wrapper_template: Option<String>,
    /// Wrap all matching call sites.
    #[arg(long)]
    pub all_calls: bool,
    /// Wrap only the call sites at these expression paths.
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Fail if no selected call site changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Require at least this many call-site rewrites.
    #[arg(long)]
    pub require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ReplaceFunctionCallsArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing callable call head.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement callable call head.
    #[arg(long)]
    pub to: SymbolName,
    /// Replace all matching call sites.
    #[arg(long)]
    pub all_calls: bool,
    /// Replace only the call sites at these expression paths.
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Fail if no selected call site changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Require at least this many call-site rewrites.
    #[arg(long)]
    pub require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct UnwrapFunctionCallsArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Existing callable head inside the wrapper.
    #[arg(long)]
    pub function: SymbolName,
    /// Wrapper callable or macro removed around each selected call.
    #[arg(long)]
    pub wrapper: SymbolName,
    /// Unwrap all matching unary wrapper call sites.
    #[arg(long)]
    pub all_calls: bool,
    /// Unwrap only the wrapper call sites at these expression paths.
    #[arg(long = "call-path")]
    pub call_paths: Vec<Path>,
    /// Rewrite changed files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Fail if no selected call site changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Require at least this many call-site rewrites.
    #[arg(long)]
    pub require_calls: Option<usize>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenameAtArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Byte offset inside the symbol atom to rename.
    #[arg(long)]
    pub at: usize,
    /// Replacement symbol atom.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Exit non-zero when no occurrence changes.
    #[arg(long)]
    pub fail_on_no_change: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
