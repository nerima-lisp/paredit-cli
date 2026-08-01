#![doc = include_str!("../README.md")]

pub mod api_diff_report;
pub mod api_surface_report;
pub mod blame_report;
pub mod external_system_report;
pub mod keyword_arity_report;
pub mod license_header_report;
pub mod license_report;
pub mod serial_consistency_report;
pub mod symbol_index_report;
pub mod symbol_report;
pub mod test_map_report;
pub mod unreachable_expression_report;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use api_diff_report::cli::{ApiDiffReportArgs, api_diff_report};
pub use api_surface_report::cli::{ApiSurfaceReportArgs, api_surface_report};
pub use blame_report::cli::{BlameReportArgs, blame_report};
pub use external_system_report::cli::{ExternalSystemReportArgs, external_system_report};
pub use keyword_arity_report::cli::{KeywordArityReportArgs, keyword_arity_report};
pub use license_header_report::cli::{LicenseHeaderReportArgs, license_header_report};
pub use license_report::cli::{LicenseReportArgs, license_report};
pub use serial_consistency_report::cli::{SerialConsistencyReportArgs, serial_consistency_report};
pub use symbol_index_report::cli::{SymbolIndexReportArgs, symbol_index_report};
pub use symbol_report::cli::{SymbolQueryArgs, SymbolReportArgs, find_symbol, symbol_report};
pub use test_map_report::cli::{TestMapReportArgs, test_map_report};
pub use unreachable_expression_report::cli::{
    UnreachableExpressionReportArgs, unreachable_expression_report,
};
