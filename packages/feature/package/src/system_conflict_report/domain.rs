//! ASDF system-identity conflict detection: two or more *distinct*
//! `asdf:defsystem` forms — in one file or across several — that declare
//! the same system name.
//!
//! Unlike Common Lisp packages, ASDF systems have no `:nicknames` option,
//! so this check has only the one shape [`crate::package_conflict_report::domain`]
//! covers as its "duplicated primary name" case: whichever `defsystem` form
//! is loaded last silently redefines the system ASDF already knows by that
//! name — a load-order-dependent bug, not a style nit.
//!
//! Scope: Common Lisp only, since ASDF `defsystem` is CL-specific. Reuses
//! the same [`paredit_core_syntax::common_lisp::normalize_common_lisp_package_designator`]
//! designator normalization `system-cycle-report` and
//! [`crate::dependency_report::domain`] already rely on for system-name
//! designators (`"my-system"`, `#:my-system`, or a bare symbol all name the
//! same system).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::PackageRefactorResult;

use paredit_core_syntax::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_child, list_head};

#[derive(Debug, Clone)]
pub struct DeclaredSystem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
}

impl DeclaredSystem {
    pub fn new(path: PathBuf, span: ByteSpan, name: impl Into<String>) -> Self {
        Self {
            path,
            span,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemConflictOccurrence {
    pub path: PathBuf,
    pub span: ByteSpan,
}

#[derive(Debug, Clone)]
pub struct SystemConflictItem {
    pub name: String,
    pub occurrences: Vec<SystemConflictOccurrence>,
}

#[derive(Debug)]
pub struct SystemConflictSummary {
    pub declared_count: usize,
    pub conflicts: Vec<SystemConflictItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SystemConflictPolicyOptions {
    fail_on_conflict: bool,
}

impl SystemConflictPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_conflict: bool) -> Self {
        Self { fail_on_conflict }
    }

    #[must_use]
    pub const fn fail_on_conflict(self) -> bool {
        self.fail_on_conflict
    }
}

#[derive(Debug)]
pub struct SystemConflictPolicy {
    pub fail_on_conflict: bool,
    pub declared_count: usize,
    pub conflict_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every `asdf:defsystem` form's own (normalized) name in one
/// file, regardless of whether it declares any `:depends-on` — unlike
/// [`crate::dependency_report::domain::build_system_dependency_edges`],
/// which only emits a `(system, target)` pair when a `:depends-on` option
/// exists, every `defsystem` form is a candidate here.
pub fn collect_declared_systems(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> PackageRefactorResult<Vec<DeclaredSystem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let mut declared = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if !dialect.is_common_lisp_asdf_system_definition_head(head) {
            continue;
        }
        let Some(name) = atom_child(&view, 1) else {
            continue;
        };
        declared.push(DeclaredSystem::new(
            path.to_path_buf(),
            view.span,
            normalize_common_lisp_package_designator(name),
        ));
    }
    Ok(declared)
}

#[must_use]
pub fn analyze_system_conflicts(declared: &[DeclaredSystem]) -> SystemConflictSummary {
    let mut groups: BTreeMap<String, Vec<&DeclaredSystem>> = BTreeMap::new();
    for system in declared {
        groups
            .entry(common_lisp_symbol_reference_needle(&system.name))
            .or_default()
            .push(system);
    }

    let mut conflicts = Vec::new();
    for group in groups.into_values() {
        if group.len() < 2 {
            continue;
        }

        conflicts.push(SystemConflictItem {
            name: group[0].name.clone(),
            occurrences: group
                .iter()
                .map(|system| SystemConflictOccurrence {
                    path: system.path.clone(),
                    span: system.span,
                })
                .collect(),
        });
    }

    SystemConflictSummary {
        declared_count: declared.len(),
        conflicts,
    }
}

#[must_use]
pub fn evaluate_system_conflict_policy(
    options: SystemConflictPolicyOptions,
    summary: &SystemConflictSummary,
) -> SystemConflictPolicy {
    let conflict_count = summary.conflicts.len();
    let mut violations = Vec::new();
    if options.fail_on_conflict() && conflict_count > 0 {
        violations.push(format!("conflict_count {conflict_count} exceeds 0"));
    }

    SystemConflictPolicy {
        fail_on_conflict: options.fail_on_conflict(),
        declared_count: summary.declared_count,
        conflict_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(path: &str, input: &str) -> Vec<DeclaredSystem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_systems(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared systems")
    }

    #[test]
    fn flags_two_distinct_files_declaring_the_same_system_name() {
        let mut declared_systems = declared("a.asd", "(asdf:defsystem \"app\" :depends-on ())");
        declared_systems.extend(declared("b.asd", "(asdf:defsystem \"app\" :depends-on ())"));

        let summary = analyze_system_conflicts(&declared_systems);
        assert_eq!(summary.conflicts.len(), 1);
        assert_eq!(summary.conflicts[0].name, "app");
        assert_eq!(summary.conflicts[0].occurrences.len(), 2);
    }

    #[test]
    fn flags_a_conflict_even_when_neither_system_has_depends_on() {
        let mut declared_systems = declared("a.asd", "(asdf:defsystem \"app\")");
        declared_systems.extend(declared("b.asd", "(asdf:defsystem #:app)"));

        let summary = analyze_system_conflicts(&declared_systems);
        assert_eq!(summary.conflicts.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_system_names() {
        let mut declared_systems = declared("a.asd", "(asdf:defsystem \"app\")");
        declared_systems.extend(declared("b.asd", "(asdf:defsystem \"lib\")"));

        let summary = analyze_system_conflicts(&declared_systems);
        assert!(summary.conflicts.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app)").expect("parse input");
        let declared_systems =
            collect_declared_systems(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect declared systems");
        assert!(declared_systems.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut declared_systems = declared("a.asd", "(asdf:defsystem \"app\")");
        declared_systems.extend(declared("b.asd", "(asdf:defsystem \"app\")"));
        let summary = analyze_system_conflicts(&declared_systems);

        let quiet =
            evaluate_system_conflict_policy(SystemConflictPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.conflict_count, 1);

        let strict =
            evaluate_system_conflict_policy(SystemConflictPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
