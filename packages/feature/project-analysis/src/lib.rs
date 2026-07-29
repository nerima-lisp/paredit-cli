#![doc = include_str!("../README.md")]

pub mod call_cycle_report;
pub mod call_graph_report;
pub mod call_report;
pub mod class_cycle_report;
pub mod complexity_report;
pub mod context_report;
pub mod error;
pub mod form_report;
pub mod impact_report;
pub mod naming_report;
pub mod package_cycle_report;
pub mod reachability_report;
pub mod redefinition_report;
pub mod signature_report;
pub mod source_report;
pub mod struct_cycle_report;
pub mod system_cycle_report;
pub mod system_order;
pub mod undefined_package_report;
pub mod unused_local_callable_report;
pub mod workspace_report;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use call_cycle_report::cli::{CallCycleReportArgs, call_cycle_report};
pub use call_graph_report::cli::{CallGraphArgs, call_graph};
pub use call_report::cli::{CallReportArgs, call_report};
pub use class_cycle_report::cli::{ClassCycleReportArgs, class_cycle_report};
pub use complexity_report::cli::{ComplexityReportArgs, complexity_report};
pub use context_report::cli::{ContextAtArgs, context_at_report};
pub use form_report::cli::{FormReportArgs, form_report};
pub use impact_report::cli::{ImpactReportArgs, impact_report};
pub use naming_report::cli::{NamingReportArgs, naming_report};
pub use package_cycle_report::cli::{PackageCycleReportArgs, package_cycle_report};
pub use reachability_report::cli::{ReachabilityReportArgs, reachability_report};
pub use redefinition_report::cli::{RedefinitionReportArgs, redefinition_report};
pub use signature_report::cli::{SignatureReportArgs, signature_report};
pub use source_report::cli::{SourceReportArgs, source_report};
pub use struct_cycle_report::cli::{StructCycleReportArgs, struct_cycle_report};
pub use system_cycle_report::cli::{SystemCycleReportArgs, system_cycle_report};
pub use undefined_package_report::cli::{UndefinedPackageReportArgs, undefined_package_report};
pub use unused_local_callable_report::cli::{
    UnusedLocalCallableReportArgs, unused_local_callable_report,
};
pub use workspace_report::cli::{WorkspaceReportArgs, workspace_report};

pub use error::{ProjectAnalysisError, ProjectAnalysisResult, WorkspaceAnalysisError};
