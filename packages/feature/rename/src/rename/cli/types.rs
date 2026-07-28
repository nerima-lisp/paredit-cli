use std::path::PathBuf;

use crate::rename::usecase::{
    RenameFunctionOccurrence, ReplaceFunctionCallSite, UnwrapFunctionCallSite, WrapFunctionCallSite,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteSpan;

#[derive(Debug)]
pub struct RenameFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub occurrences: Vec<ByteSpan>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

/// Shared per-file report for the callable rename family
/// (rename-function, rename-macrolet, rename-local-function).
#[derive(Debug)]
pub struct CallableRenameFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub calls: Vec<RenameFunctionOccurrence>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct RenameSymbolMacroFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub references: Vec<RenameFunctionOccurrence>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

/// Shared pre-write state for the callable rename family.
#[derive(Debug)]
pub struct PendingCallableRenameFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub calls: Vec<RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug)]
pub struct PendingRenameSymbolMacroFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub references: Vec<RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug)]
pub struct WrapFunctionCallsFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<WrapFunctionCallSite>,
    pub skipped_already_wrapped: Vec<WrapFunctionCallSite>,
    pub skipped_nested: Vec<WrapFunctionCallSite>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct PendingWrapFunctionCallsFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<WrapFunctionCallSite>,
    pub skipped_already_wrapped: Vec<WrapFunctionCallSite>,
    pub skipped_nested: Vec<WrapFunctionCallSite>,
    pub rewritten: String,
    pub changed: bool,
}

/// Shared policy outcome for the wrap/replace/unwrap call-site commands.
#[derive(Debug)]
pub struct CallSitePolicy {
    pub fail_on_no_change: bool,
    pub require_calls: Option<usize>,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[derive(Debug)]
pub struct ReplaceFunctionCallsFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<ReplaceFunctionCallSite>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct PendingReplaceFunctionCallsFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<ReplaceFunctionCallSite>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug)]
pub struct UnwrapFunctionCallsFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<UnwrapFunctionCallSite>,
    pub skipped_non_unary_wrapper: Vec<UnwrapFunctionCallSite>,
    pub skipped_nested: Vec<UnwrapFunctionCallSite>,
    pub changed: bool,
    pub written: bool,
    pub rewritten: String,
}

#[derive(Debug)]
pub struct PendingUnwrapFunctionCallsFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<UnwrapFunctionCallSite>,
    pub skipped_non_unary_wrapper: Vec<UnwrapFunctionCallSite>,
    pub skipped_nested: Vec<UnwrapFunctionCallSite>,
    pub rewritten: String,
    pub changed: bool,
}
