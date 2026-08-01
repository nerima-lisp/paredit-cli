//! PackageLevelShadowing (an inner let binding or lambda-list parameter that reuses the name of a top-level defun/defvar/defparameter/defconstant/defmacro in the same file) detection.

pub use crate::package_level_shadowing::domain::{
    PackageLevelShadowingItem, PackageLevelShadowingPolicy, PackageLevelShadowingPolicyOptions,
    PackageLevelShadowingSummary, collect_package_level_shadowing,
    evaluate_package_level_shadowing_policy, summarize_package_level_shadowing,
};
