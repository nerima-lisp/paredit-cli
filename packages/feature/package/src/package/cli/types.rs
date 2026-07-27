use std::path::PathBuf;

use crate::application::usecase::package::{self as package_usecase, PackageRenameOccurrence};
use crate::application::usecase::package_report::PackageReport as ApplicationPackageReport;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, SymbolName};
use crate::presentation::cli::args::{DialectArg, OutputFormat};

use clap::{Args, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum PackageOptionOrderArg {
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
pub(in crate::presentation::cli) struct PackageReportArgs {
    /// Files to scan.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct AddExportArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub(super) file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Package name to update. Required when the file contains multiple defpackage forms.
    #[arg(long)]
    pub(super) package: Option<SymbolName>,
    /// Symbol atom to add to the :export option.
    #[arg(long)]
    pub(super) symbol: SymbolName,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct RenamePackageArgs {
    /// Files to scan and optionally rewrite.
    #[arg(required = true)]
    pub(super) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Source package name or designator, for example old.pkg or #:old.pkg.
    #[arg(long)]
    pub(super) from: SymbolName,
    /// Replacement package name. Prefix edits use the normalized package name.
    #[arg(long)]
    pub(super) to: SymbolName,
    /// Rewrite changed input files in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SortPackageExportsArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub(super) file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Package name to update. Without this flag, all defpackage :export options are sorted.
    #[arg(long)]
    pub(super) package: Option<SymbolName>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SortPackageOptionsArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub(super) file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Package name to update. Without this flag, all defpackage option forms are sorted.
    #[arg(long)]
    pub(super) package: Option<SymbolName>,
    /// Option ordering strategy.
    #[arg(long, value_enum, default_value_t = PackageOptionOrderArg::Canonical)]
    pub(super) order: PackageOptionOrderArg,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct MergePackageOptionsArgs {
    /// Package definition file to scan and optionally rewrite.
    #[arg(short, long)]
    pub(super) file: PathBuf,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Package name to update. Without this flag, all defpackage option forms are merged.
    #[arg(long)]
    pub(super) package: Option<SymbolName>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    pub(super) write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(super) output: OutputFormat,
}

#[derive(Debug)]
pub(super) struct PackageReportFile {
    pub(super) path: PathBuf,
    pub(super) dialect: Dialect,
    pub(super) report: ApplicationPackageReport,
}

#[derive(Debug)]
pub(super) struct AddExportPlan {
    pub(super) path: PathBuf,
    pub(super) dialect: Dialect,
    pub(super) package: String,
    pub(super) symbol: SymbolName,
    pub(super) defpackage_path: String,
    pub(super) defpackage_span: ByteSpan,
    pub(super) export_span: Option<ByteSpan>,
    pub(super) insertion_span: ByteSpan,
    pub(super) already_exported: bool,
    pub(super) changed: bool,
    pub(super) written: bool,
    pub(super) rewritten: String,
}

#[derive(Debug)]
pub(super) struct RenamePackageFilePlan {
    pub(super) path: PathBuf,
    pub(super) dialect: Dialect,
    pub(super) occurrences: Vec<PackageRenameOccurrence>,
    pub(super) changed: bool,
    pub(super) written: bool,
}

#[derive(Debug)]
pub(super) struct SortPackageExportsPlan {
    pub(super) path: PathBuf,
    pub(super) dialect: Dialect,
    pub(super) exports: Vec<package_usecase::PackageExportSort>,
    pub(super) changed: bool,
    pub(super) written: bool,
    pub(super) rewritten: String,
}

#[derive(Debug)]
pub(super) struct SortPackageOptionsPlan {
    pub(super) path: PathBuf,
    pub(super) dialect: Dialect,
    pub(super) packages: Vec<package_usecase::PackageOptionSort>,
    pub(super) changed: bool,
    pub(super) written: bool,
    pub(super) rewritten: String,
}

#[derive(Debug)]
pub(super) struct MergePackageOptionsPlan {
    pub(super) path: PathBuf,
    pub(super) dialect: Dialect,
    pub(super) merges: Vec<package_usecase::PackageOptionMerge>,
    pub(super) changed: bool,
    pub(super) written: bool,
    pub(super) rewritten: String,
}
