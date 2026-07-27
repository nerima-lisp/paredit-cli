pub mod args;
pub mod insert_top_level;
pub mod move_definition;
pub mod move_form;
mod render;
mod shared;
pub mod sort_definitions;
pub mod split_file;
mod types;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of each subcommand this slice owns.
pub use args::{
    InsertTopLevelArgs, MoveDefinitionArgs, MoveFormArgs, SortDefinitionsArgs, SplitFileArgs,
};
pub use insert_top_level::insert_top_level;
pub use move_definition::move_definition;
pub use move_form::move_form;
pub use sort_definitions::sort_definitions;
pub use split_file::split_file;
