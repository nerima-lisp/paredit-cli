//! Common Lisp definition-redefinition detection: two or more top-level
//! definitions of the same category and name declared in the same package —
//! across one file or several — a common source of "; WARNING: redefining
//! FOO in DEFUN" surprises at load time, and often a straightforward
//! copy-paste or merge-conflict bug rather than an intentional one.
//!
//! Built on the same [`paredit_feature_remove_unused::definition_report::domain::collect_definition_forms`]
//! extraction [`paredit_feature_remove_unused::definition_report::domain`]'s own unused-definition
//! check reuses, read across files instead of within one.
//!
//! Scope: Common Lisp only — grouping by "declaring package" requires the
//! `in-package`-tracked package this codebase already collects for CL;
//! other dialects have no equivalent namespace tracking here, so treating
//! same-named definitions from unrelated modules as a collision would be a
//! false positive. [`paredit_core_syntax::definition::DefinitionCategory::Method`]
//! is excluded: CLOS generic functions are expected to gather several
//! `defmethod` forms under the same name with different specializers —
//! that's normal dispatch, not a redefinition bug.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::ProjectAnalysisResult;

use paredit_core_syntax::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use paredit_core_syntax::definition::DefinitionCategory;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_feature_remove_unused::definition_report::domain::collect_definition_forms;

#[derive(Debug, Clone)]
pub struct DeclaredDefinition {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: Option<String>,
    pub category: DefinitionCategory,
    pub name: String,
}

impl DeclaredDefinition {
    pub fn new(
        path: PathBuf,
        span: ByteSpan,
        package: Option<String>,
        category: DefinitionCategory,
        name: impl Into<String>,
    ) -> Self {
        Self {
            path,
            span,
            package,
            category,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedefinitionOccurrence {
    pub path: PathBuf,
    pub span: ByteSpan,
}

#[derive(Debug, Clone)]
pub struct RedefinitionItem {
    pub package: Option<String>,
    pub category: DefinitionCategory,
    pub name: String,
    pub occurrences: Vec<RedefinitionOccurrence>,
}

#[derive(Debug)]
pub struct RedefinitionSummary {
    pub declared_count: usize,
    pub redefinitions: Vec<RedefinitionItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedefinitionPolicyOptions {
    fail_on_redefinition: bool,
}

impl RedefinitionPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_redefinition: bool) -> Self {
        Self {
            fail_on_redefinition,
        }
    }

    #[must_use]
    pub const fn fail_on_redefinition(self) -> bool {
        self.fail_on_redefinition
    }
}

#[derive(Debug)]
pub struct RedefinitionPolicy {
    pub fail_on_redefinition: bool,
    pub declared_count: usize,
    pub redefinition_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every named, non-`Method` top-level definition in one file,
/// paired with its (normalized) declaring package, if any.
pub fn collect_declared_definitions(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> ProjectAnalysisResult<Vec<DeclaredDefinition>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let (_, definitions) = collect_definition_forms(tree, dialect)?;
    Ok(definitions
        .into_iter()
        .filter(|item| item.category != DefinitionCategory::Method)
        .filter_map(|item| {
            let name = item.name?;
            let package = item
                .package
                .as_deref()
                .map(|package| normalize_common_lisp_package_designator(package).to_owned());
            Some(DeclaredDefinition::new(
                path.to_path_buf(),
                item.span,
                package,
                item.category,
                name,
            ))
        })
        .collect())
}

pub fn analyze_redefinitions(declared: &[DeclaredDefinition]) -> RedefinitionSummary {
    let mut groups: BTreeMap<
        (Option<String>, DefinitionCategory, String),
        Vec<&DeclaredDefinition>,
    > = BTreeMap::new();
    for definition in declared {
        let package_needle = definition
            .package
            .as_deref()
            .map(common_lisp_symbol_reference_needle);
        let name_needle = common_lisp_symbol_reference_needle(&definition.name);
        groups
            .entry((package_needle, definition.category, name_needle))
            .or_default()
            .push(definition);
    }

    let mut redefinitions = Vec::new();
    for group in groups.into_values() {
        if group.len() < 2 {
            continue;
        }

        redefinitions.push(RedefinitionItem {
            package: group[0].package.clone(),
            category: group[0].category,
            name: group[0].name.clone(),
            occurrences: group
                .iter()
                .map(|definition| RedefinitionOccurrence {
                    path: definition.path.clone(),
                    span: definition.span,
                })
                .collect(),
        });
    }

    RedefinitionSummary {
        declared_count: declared.len(),
        redefinitions,
    }
}

#[must_use]
pub fn evaluate_redefinition_policy(
    options: RedefinitionPolicyOptions,
    summary: &RedefinitionSummary,
) -> RedefinitionPolicy {
    let redefinition_count = summary.redefinitions.len();
    let mut violations = Vec::new();
    if options.fail_on_redefinition() && redefinition_count > 0 {
        violations.push(format!("redefinition_count {redefinition_count} exceeds 0"));
    }

    RedefinitionPolicy {
        fail_on_redefinition: options.fail_on_redefinition(),
        declared_count: summary.declared_count,
        redefinition_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(path: &str, input: &str) -> Vec<DeclaredDefinition> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_definitions(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared definitions")
    }

    #[test]
    fn flags_two_functions_with_the_same_name_in_the_same_package() {
        let mut declared_definitions = declared("a.lisp", "(defun foo () 1)");
        declared_definitions.extend(declared("b.lisp", "(defun foo () 2)"));

        let summary = analyze_redefinitions(&declared_definitions);
        assert_eq!(summary.redefinitions.len(), 1);
        assert_eq!(summary.redefinitions[0].name, "foo");
        assert_eq!(summary.redefinitions[0].occurrences.len(), 2);
    }

    #[test]
    fn does_not_flag_the_same_name_in_different_packages() {
        let mut declared_definitions = declared("a.lisp", "(in-package :a)\n(defun foo () 1)");
        declared_definitions.extend(declared("b.lisp", "(in-package :b)\n(defun foo () 2)"));

        let summary = analyze_redefinitions(&declared_definitions);
        assert!(summary.redefinitions.is_empty());
    }

    #[test]
    fn does_not_flag_different_categories_with_the_same_name() {
        let mut declared_definitions = declared("a.lisp", "(defun foo () 1)");
        declared_definitions.extend(declared("b.lisp", "(defvar foo 2)"));

        let summary = analyze_redefinitions(&declared_definitions);
        assert!(summary.redefinitions.is_empty());
    }

    #[test]
    fn does_not_flag_multiple_methods_with_the_same_name() {
        let mut declared_definitions = declared("a.lisp", "(defmethod area ((shape circle)) 1)");
        declared_definitions.extend(declared("b.lisp", "(defmethod area ((shape square)) 2)"));

        let summary = analyze_redefinitions(&declared_definitions);
        assert!(summary.redefinitions.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defn foo [] 1)").expect("parse input");
        let declared_definitions =
            collect_declared_definitions(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect declared definitions");
        assert!(declared_definitions.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut declared_definitions = declared("a.lisp", "(defun foo () 1)");
        declared_definitions.extend(declared("b.lisp", "(defun foo () 2)"));
        let summary = analyze_redefinitions(&declared_definitions);

        let quiet = evaluate_redefinition_policy(RedefinitionPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.redefinition_count, 1);

        let strict = evaluate_redefinition_policy(RedefinitionPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
