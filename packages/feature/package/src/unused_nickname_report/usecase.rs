//! Cross-file unused `defpackage` `:nicknames` detection across explicit files.

pub use crate::domain::unused_nickname_report::{
    DeclaredNickname, UnusedNicknameItem, UnusedNicknamePolicy, UnusedNicknamePolicyOptions,
    UnusedNicknameSummary, analyze_unused_nicknames, collect_declared_nicknames,
    collect_referenced_package_names, evaluate_unused_nickname_policy,
};
