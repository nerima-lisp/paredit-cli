pub mod args;
pub mod workflow;

mod render;

// Hoisted for the composition root (section 4.2).
pub use args::MigrateCommand;
pub use workflow::migrate;
