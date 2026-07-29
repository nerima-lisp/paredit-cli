use std::path::PathBuf;

use clap::Args;

use paredit_core_cli::args::OutputFormat;

/// How to find the configuration, shared by every `config` subcommand.
///
/// Flattened rather than repeated so that `check` and `show` cannot drift into
/// looking at different files — which would make `check` a useless check.
#[derive(Debug, Args, Clone, Default)]
pub struct ConfigLocationArgs {
    /// Read this file instead of discovering one. Replaces the user,
    /// repository, and directory layers.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Ignore every configuration file, leaving the built-in defaults.
    #[arg(long, conflicts_with = "config")]
    pub no_config: bool,
    /// Ignore the PAREDIT_* environment overrides.
    #[arg(long)]
    pub no_config_env: bool,
    /// Resolve discovery from this directory instead of the working directory.
    #[arg(long, value_name = "DIR")]
    pub from: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ConfigCheckArgs {
    #[command(flatten)]
    pub location: ConfigLocationArgs,
    /// Exit zero even when the configuration has errors, and report them anyway.
    #[arg(long)]
    pub no_fail: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ConfigShowArgs {
    #[command(flatten)]
    pub location: ConfigLocationArgs,
    /// Show only this key.
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,
    /// Show only the keys some layer actually set, hiding the defaults.
    #[arg(long)]
    pub changed_only: bool,
    /// Also report the flags this configuration would add to one command, as
    /// a quoted path: --for "inspect lint".
    #[arg(long = "for", value_name = "COMMAND")]
    pub for_command: Option<String>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ConfigSchemaArgs {
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    /// Where to write the file. Defaults to `paredit.toml` in the working
    /// directory, or in the repository root with --repository-root.
    #[arg(long, value_name = "FILE")]
    pub path: Option<PathBuf>,
    /// Write to the repository root rather than the working directory.
    #[arg(long, conflicts_with = "path")]
    pub repository_root: bool,
    /// Write every key, including the ones already at their default.
    #[arg(long)]
    pub all_keys: bool,
    /// Output format for agent consumption. Ignored with the global
    /// --dry-run, whose payload is the file itself.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}
