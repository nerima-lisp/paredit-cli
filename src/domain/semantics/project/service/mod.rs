//! Resolving packages and assembling the project-wide table.

mod global_builder;
mod package_resolver;
mod system_order;

pub use global_builder::{ProjectFile, build_global_table};
pub use package_resolver::{
    FilePackages, PackageRegion, canonical_package_id, resolve_file_packages, resolve_symbol,
};
pub use system_order::{SystemOrderCycle, resolve_system_order, system_dependency_edges};
