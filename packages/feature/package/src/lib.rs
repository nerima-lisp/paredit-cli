#![doc = include_str!("../README.md")]

pub mod dependency_report;
pub mod duplicate_export_report;
pub mod error;
pub mod package;
pub mod package_boundary_report;
pub mod package_conflict_report;
pub mod package_report;
pub mod system_conflict_report;
pub mod unused_export_report;
pub mod unused_nickname_report;
pub mod unused_package_report;
pub mod use_widening_report;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use duplicate_export_report::cli::{DuplicateExportReportArgs, duplicate_export_report};
pub use package::cli::{AddExportArgs, add_export};
pub use package::cli::{MergePackageOptionsArgs, merge_package_options};
pub use package::cli::{PackageReportArgs, package_report};
pub use package::cli::{RenamePackageArgs, rename_package};
pub use package::cli::{SortPackageExportsArgs, sort_package_exports};
pub use package::cli::{SortPackageOptionsArgs, sort_package_options};
pub use package_boundary_report::cli::{PackageBoundaryReportArgs, package_boundary_report};
pub use package_conflict_report::cli::{PackageConflictReportArgs, package_conflict_report};
pub use system_conflict_report::cli::{SystemConflictReportArgs, system_conflict_report};
pub use unused_export_report::cli::{UnusedExportReportArgs, unused_export_report};
pub use unused_nickname_report::cli::{UnusedNicknameReportArgs, unused_nickname_report};
pub use unused_package_report::cli::{UnusedPackageReportArgs, unused_package_report};
pub use use_widening_report::cli::{UseWideningReportArgs, use_widening_report};

pub use error::{
    DefpackageSelectionError, DefpackageShapeError, PackageRefactorError, PackageRefactorResult,
};
