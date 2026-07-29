#![doc = include_str!("../README.md")]

pub mod circular_literal_report;
pub mod class_hierarchy_report;
pub mod duplicate_method_report;
pub mod duplicate_slot_report;
pub mod format_directive_report;
pub mod generic_dispatch_report;
pub mod loop_report;
pub mod macro_expansion_report;
pub mod macro_hygiene_report;
pub mod method_combination_report;
pub mod package_lock_report;
pub mod read_conditional_report;
pub mod read_time_eval_report;
pub mod readtable_case_report;
pub mod restart_report;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use circular_literal_report::cli::{CircularLiteralReportArgs, circular_literal_report};
pub use class_hierarchy_report::cli::{ClassHierarchyReportArgs, class_hierarchy_report};
pub use duplicate_method_report::cli::{DuplicateMethodReportArgs, duplicate_method_report};
pub use duplicate_slot_report::cli::{DuplicateSlotReportArgs, duplicate_slot_report};
pub use format_directive_report::cli::{FormatDirectiveReportArgs, format_directive_report};
pub use generic_dispatch_report::cli::{GenericDispatchReportArgs, generic_dispatch_report};
pub use loop_report::cli::{LoopReportArgs, loop_report};
pub use macro_expansion_report::cli::{MacroExpansionReportArgs, macro_expansion_report};
pub use macro_hygiene_report::cli::{MacroHygieneReportArgs, macro_hygiene_report};
pub use method_combination_report::cli::{MethodCombinationReportArgs, method_combination_report};
pub use package_lock_report::cli::{PackageLockReportArgs, package_lock_report};
pub use read_conditional_report::cli::{ReadConditionalReportArgs, read_conditional_report};
pub use read_time_eval_report::cli::{ReadTimeEvalReportArgs, read_time_eval_report};
pub use readtable_case_report::cli::{ReadtableCaseReportArgs, readtable_case_report};
pub use restart_report::cli::{RestartReportArgs, restart_report};
