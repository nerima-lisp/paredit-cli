#![doc = include_str!("../README.md")]

pub mod convert_flet_to_labels;
pub mod convert_labels_to_flet;
pub mod convert_let_star_to_let;
pub mod convert_let_to_let_star;
pub mod convert_sequential_binding;
pub mod eliminate_empty_binding_form;
pub mod error;
pub mod flatten_progn;
pub mod introduce_let;
pub mod let_report;
pub mod merge_nested_flet;
pub mod merge_nested_let;
pub mod merge_nested_let_star;
pub mod shadowed_binding_report;
pub mod split_let;
pub mod split_let_star;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use convert_flet_to_labels::cli::{ConvertFletToLabelsArgs, convert_flet_to_labels};
pub use convert_labels_to_flet::cli::{ConvertLabelsToFletArgs, convert_labels_to_flet};
pub use convert_let_star_to_let::cli::{ConvertLetStarToLetArgs, convert_let_star_to_let};
pub use convert_let_to_let_star::cli::{ConvertLetToLetStarArgs, convert_let_to_let_star};
pub use eliminate_empty_binding_form::cli::{
    EliminateEmptyBindingFormArgs, eliminate_empty_binding_form,
};
pub use flatten_progn::cli::{FlattenPrognArgs, flatten_progn};
pub use introduce_let::cli::{IntroduceLetArgs, introduce_let};
pub use let_report::cli::{LetReportArgs, let_report};
pub use merge_nested_flet::cli::{MergeNestedFletArgs, merge_nested_flet};
pub use merge_nested_let::cli::{MergeNestedLetArgs, merge_nested_let};
pub use merge_nested_let_star::cli::{MergeNestedLetStarArgs, merge_nested_let_star};
pub use shadowed_binding_report::cli::{ShadowedBindingReportArgs, shadowed_binding_report};
pub use split_let::cli::{SplitLetArgs, split_let};
pub use split_let_star::cli::{SplitLetStarArgs, split_let_star};

pub use error::{
    BindingCaptureError, BindingContextError, BindingError, BindingFormShapeError, BindingResult,
};
