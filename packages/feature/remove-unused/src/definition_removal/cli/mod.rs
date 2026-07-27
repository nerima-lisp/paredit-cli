pub mod args;
pub mod remove_definition;
pub mod remove_unused_definitions;
mod render;
mod types;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of each subcommand this slice owns.
pub use args::{RemoveDefinitionArgs, RemoveUnusedDefinitionsArgs};
pub use remove_definition::remove_definition;
pub use remove_unused_definitions::remove_unused_definitions;
