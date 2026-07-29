//! Ways of naming a form other than a tree path.
//!
//! `--path 0.2.1` and `--at 120` are exact and cheap to resolve, and both cost
//! the caller a round trip to build: an agent has to run `inspect outline`,
//! read a path out of it, and hope nothing moved in between. This module is
//! the rest of the vocabulary — name, coordinate, pattern, id, range, and
//! relative moves — resolved through one entry point, [`resolve::resolve`].
//!
//! | selector | resolved by |
//! | --- | --- |
//! | `--path`, `--at` | the tree directly |
//! | `--line-column 12:5` | [`line_index`] |
//! | `--name parse-header` | definition shapes |
//! | `--query '(defun ?name ...)'` | [`pattern`] and [`matcher`] |
//! | `--id sel:…` | [`stable_id`] |
//! | `--from` / `--to` | a compact selector per endpoint |
//! | `--parent`, `--child`, `--sibling` | relative moves over any of the above |
//!
//! The pattern language is deliberately a separate module with no CLI
//! knowledge: a custom lint-rule DSL wants exactly the same matcher, and
//! building it inside the selector would make that a rewrite rather than a
//! second caller.

pub mod error;
pub mod line_index;
pub mod matcher;
mod normalize;
pub mod pattern;
pub mod resolve;
pub mod rewrite;
pub mod stable_id;

pub use error::{PatternError, RewriteError, SelectorError, SelectorResult};
pub use line_index::{LineIndex, LinePosition};
pub use matcher::{Capture, PatternMatch, match_all};
pub use normalize::normalized_form_text;
pub use pattern::{CaptureKind, Pattern};
pub use resolve::{
    RangeExtent, RelativeStep, SelectorRequest, SelectorTarget, SelectorTerm, resolve, target_text,
};
pub use rewrite::{
    Replacement, RewriteAllowances, RewritePlan, SkipReason, SkippedMatch, Template, apply_plan,
    plan_rewrite,
};
pub use stable_id::{STABLE_ID_PREFIX, StableSelectorId, stable_id_for_path, stable_selector_ids};
