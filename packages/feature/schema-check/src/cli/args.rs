use std::path::PathBuf;

use clap::{Args, Subcommand};

use paredit_core_cli::args::ReportFormat;
use paredit_core_cli::runtime::Verbosity;

/// The `schema` namespace.
#[derive(Debug, Subcommand)]
pub enum SchemaCommand {
    /// Validate an instance file against a `defschema` schema.
    Check(SchemaCheckArgs),
}

#[derive(Debug, Args)]
#[command(
    after_help = "The `defschema` DSL is Lisp forms that are never evaluated: a schema file \
holds only field declarations, and a :type or refinement keyword this build does not recognize is a \
parse error rather than code it tries to run. `:matches` is a small glob (`*` = any run of \
characters, `?` = one character), not a regular expression.\n\n\
Examples:\n  \
paredit schema check instance.lisp --schema .paredit/schemas/config.lisp\n  \
paredit schema check instance.lisp --schema schemas.lisp --schema-name config\n  \
paredit schema check instance.lisp --schema schemas.lisp --output text"
)]
pub struct SchemaCheckArgs {
    /// The instance file to validate: an alist- or plist-shaped S-expression.
    pub instance: PathBuf,
    /// A schema file: one or more `(defschema ...)` forms, conventionally
    /// under `.paredit/schemas/*.lisp` (though any path is accepted).
    #[arg(long, value_name = "FILE")]
    pub schema: PathBuf,
    /// Which schema in `--schema` to validate against, when that file
    /// defines more than one. Optional when the file defines exactly one.
    #[arg(long, value_name = "NAME")]
    pub schema_name: Option<String>,
    /// Exit with failure when the instance has any finding.
    #[arg(long)]
    pub fail_on_violation: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    pub output: ReportFormat,
    /// How much detail the report includes.
    #[arg(long, value_enum, default_value_t = Verbosity::Normal)]
    pub verbosity: Verbosity,
}
