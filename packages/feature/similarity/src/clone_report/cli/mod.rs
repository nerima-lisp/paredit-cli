pub mod args;
pub mod collect;
pub mod git;
pub mod render;
pub mod workflow;

// The contract with the composition root (section 4.2): each command's `clap`
// argument type and the function that runs it.
pub use args::{
    CloneClassReportArgs, CloneExternalReportArgs, CloneGenealogyReportArgs,
    CloneSequenceReportArgs, CloneThresholdReportArgs,
};
pub use workflow::{
    clone_classes, clone_external, clone_genealogy, clone_sequences, clone_threshold,
};
