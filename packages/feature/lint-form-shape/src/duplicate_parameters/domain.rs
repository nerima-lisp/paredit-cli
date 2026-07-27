//! Common Lisp duplicate-parameter detection: a `defun`, `defmacro`,
//! `defmethod`, or other callable definition whose lambda list names the same
//! variable more than once — `(defun f (x y x) ...)`. The standard forbids a
//! variable appearing more than once in a lambda list, so this is always an
//! error (caught at compile time, if at all), never intentional. The report
//! therefore has no false positives.
//!
//! Reuses the same lambda-list extraction the parameter refactors and
//! [`crate::domain::unused_parameter_report`] rely on
//! ([`paredit_feature_function_parameter::function_parameter::domain::list_lambda_list_parameter_names`]),
//! so it inherits their handling of lambda-list keywords (`&optional`,
//! `&rest`, `&key`, `&aux`), default-value forms, and `&key`-with-supplied-p
//! triples — a name is compared once, however it is declared.
//!
//! Scope: Common Lisp only, and the top-level callable definitions
//! [`paredit_core_syntax::definition::definition_shape`] recognizes (matching
//! `unused-parameters`' scope).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use paredit_feature_function_parameter::function_parameter::domain::list_lambda_list_parameter_names;

#[derive(Debug, Clone)]
pub struct DuplicateParameterItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub definition: String,
    pub parameter: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateParameterSummary {
    pub definition_count: usize,
    pub duplicates: Vec<DuplicateParameterItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateParameterPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateParameterPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateParameterPolicy {
    pub fail_on_duplicate: bool,
    pub definition_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every duplicated lambda-list parameter from every callable
/// definition in one file, along with the number of definitions scanned.
pub fn collect_duplicate_parameters(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateParameterItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut definition_count = 0;
    let mut duplicates = Vec::new();

    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        if !shape.category.is_callable() {
            continue;
        }
        let Some(lambda_list) = shape.lambda_list(&view) else {
            continue;
        };
        // A lambda list this parser cannot classify is skipped rather than
        // failing the scan — one unusual macro DSL must not block the rest.
        let Ok(parameter_names) = list_lambda_list_parameter_names(dialect, lambda_list) else {
            continue;
        };
        definition_count += 1;

        let definition = shape.name(&view).unwrap_or("?").to_owned();

        // Preserve first-seen spelling and declaration order while counting.
        let mut order: Vec<String> = Vec::new();
        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for name in &parameter_names {
            let needle = common_lisp_symbol_reference_needle(name);
            let entry = counts.entry(needle.clone()).or_insert_with(|| {
                order.push(needle);
                (name.clone(), 0)
            });
            entry.1 += 1;
        }

        for needle in order {
            let (parameter, occurrence_count) = &counts[&needle];
            if *occurrence_count < 2 {
                continue;
            }
            duplicates.push(DuplicateParameterItem {
                path: path.to_path_buf(),
                span: view.span,
                definition: definition.clone(),
                parameter: parameter.clone(),
                occurrence_count: *occurrence_count,
            });
        }
    }

    Ok((definition_count, duplicates))
}

#[must_use]
pub const fn summarize_duplicate_parameters(
    definition_count: usize,
    duplicates: Vec<DuplicateParameterItem>,
) -> DuplicateParameterSummary {
    DuplicateParameterSummary {
        definition_count,
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_parameter_policy(
    options: DuplicateParameterPolicyOptions,
    summary: &DuplicateParameterSummary,
) -> DuplicateParameterPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateParameterPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        definition_count: summary.definition_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateParameterItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_parameters(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate parameters")
    }

    #[test]
    fn flags_a_parameter_declared_twice() {
        let (definition_count, duplicates) = duplicates("(defun f (x y x) (+ x y))");
        assert_eq!(definition_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].parameter, "x");
        assert_eq!(duplicates[0].occurrence_count, 2);
        assert_eq!(duplicates[0].definition, "f");
    }

    #[test]
    fn folds_symbol_case() {
        let (_, duplicates) = duplicates("(defun f (arg ARG) arg)");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn flags_a_duplicate_across_a_lambda_list_keyword() {
        let (_, duplicates) = duplicates("(defun f (x &optional x) x)");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_parameters() {
        let (definition_count, duplicates) = duplicates("(defun f (x y &optional z) (+ x y z))");
        assert_eq!(definition_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_confuse_parameters_of_two_definitions() {
        let (definition_count, duplicates) = duplicates("(defun f (x) x)\n(defun g (x) x)");
        assert_eq!(definition_count, 2);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defun f (x x) x)").expect("parse input");
        let (definition_count, duplicates) =
            collect_duplicate_parameters(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate parameters");
        assert_eq!(definition_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (definition_count, items) = duplicates("(defun f (x x) x)");
        let summary = summarize_duplicate_parameters(definition_count, items);

        let quiet = evaluate_duplicate_parameter_policy(
            DuplicateParameterPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict = evaluate_duplicate_parameter_policy(
            DuplicateParameterPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
