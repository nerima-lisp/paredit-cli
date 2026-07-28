pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::ClassHierarchyReportArgs;
pub use workflow::class_hierarchy_report;
