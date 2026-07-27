//! Refactor planning, preview, and guarded apply services.
//!
//! These workflows keep Lisp scope and macro boundaries intact while turning
//! a requested edit into plan, preview, and execution stages.

pub mod execute;
pub mod plan;
pub mod preview;
