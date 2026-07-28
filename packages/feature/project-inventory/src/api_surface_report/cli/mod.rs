pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::ApiSurfaceReportArgs;
pub use workflow::api_surface_report;
