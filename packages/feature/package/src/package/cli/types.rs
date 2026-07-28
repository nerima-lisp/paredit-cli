use std::path::PathBuf;

use crate::package::usecase::{self as package_usecase, PackageRenameOccurrence};
use crate::package_report::usecase::PackageReport as ApplicationPackageReport;
use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SymbolName};

use clap::{Args, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PackageOptionOrderArg {
    Canonical,
    Name,
}

impl From<PackageOptionOrderArg> for package_usecase::PackageOptionSortOrder {
    fn from(value: PackageOptionOrderArg) -> Self {
        match value {
            PackageOptionOrderArg::Canonical => Self::Canonical,
            PackageOptionOrderArg::Name => Self::Name,
        }
    }
}

#[derive(Debug, Args)]
pub struct PackageReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct AddExportArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Package name to update. Required when the file contains multiple defpackage forms.
    #[arg(long)]
    pub package: Option<SymbolName>,
    /// Symbol atom to add to the :export option.
    #[arg(long)]
    pub symbol: SymbolName,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RenamePackageArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Source package name or designator, for example old.pkg or #:old.pkg.
    #[arg(long)]
    pub from: SymbolName,
    /// Replacement package name. Prefix edits use the normalized package name.
    #[arg(long)]
    pub to: SymbolName,
    /// Rewrite changed input files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct SortPackageExportsArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Package name to update. Without this flag, all defpackage :export options are sorted.
    #[arg(long)]
    pub package: Option<SymbolName>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct SortPackageOptionsArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Package name to update. Without this flag, all defpackage option forms are sorted.
    #[arg(long)]
    pub package: Option<SymbolName>,
    /// Option ordering strategy.
    #[arg(long, value_enum, default_value_t = PackageOptionOrderArg::Canonical)]
    pub order: PackageOptionOrderArg,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct MergePackageOptionsArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Package name to update. Without this flag, all defpackage option forms are merged.
    #[arg(long)]
    pub package: Option<SymbolName>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

#[derive(Debug)]
pub struct PackageReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub report: ApplicationPackageReport,
}

#[derive(Debug)]
pub struct AddExportPlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: String,
    pub symbol: SymbolName,
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub export_span: Option<ByteSpan>,
    pub insertion_span: ByteSpan,
    pub already_exported: bool,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct RenamePackageFilePlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub occurrences: Vec<PackageRenameOccurrence>,
    pub changed: bool,
    pub written: bool,
}

#[derive(Debug)]
pub struct SortPackageExportsPlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub exports: Vec<package_usecase::PackageExportSort>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct SortPackageOptionsPlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub packages: Vec<package_usecase::PackageOptionSort>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct MergePackageOptionsPlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub merges: Vec<package_usecase::PackageOptionMerge>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}
