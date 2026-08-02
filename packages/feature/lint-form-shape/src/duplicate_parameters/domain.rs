//! Common Lisp duplicate-parameter detection: a `defun`, `defmacro`,
//! `defmethod`, or other callable definition whose lambda list names the same
//! variable more than once — `(defun f (x y x) ...)`. The standard forbids a
//! variable appearing more than once in a lambda list, so this is always an
//! error (caught at compile time, if at all), never intentional. The report
//! therefore has no false positives.
//!
//! Reuses the same lambda-list extraction the parameter refactors and
//! the unused-parameter report rely on
//! ([`paredit_feature_function_parameter::function_parameter::domain::list_lambda_list_parameter_names`]),
//! so it inherits their handling of lambda-list keywords (`&optional`,
//! `&rest`, `&key`, `&aux`), default-value forms, and `&key`-with-supplied-p
//! triples — a name is compared once, however it is declared.
//!
//! Scope: Common Lisp only, and the top-level callable definitions
//! [`paredit_core_syntax::definition::definition_shape`] recognizes (matching
//! `unused-parameters`' scope).

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use paredit_feature_function_parameter::function_parameter::domain::list_lambda_list_parameter_names;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct DuplicateParameterItem {
    pub span: ByteSpan,
    pub definition: String,
    pub parameter: String,
    pub occurrence_count: usize,
}

impl Finding for DuplicateParameterItem {
    /// The rule's own name, not the parameter.
    ///
    /// The parameter is read from the source and so is an open set, while
    /// `kind` is `&'static str`. It stays a text column and a JSON field
    /// instead.
    fn kind(&self) -> &'static str {
        "duplicate-parameters"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("definition={}", self.definition),
            format!("parameter={}", self.parameter),
            format!("count={}", self.occurrence_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("definition", json!(self.definition)),
            ("parameter", json!(self.parameter)),
            ("occurrence_count", json!(self.occurrence_count)),
        ]
    }

    /// The same sentence the `duplicate-parameters` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one defect described one way.
    fn message(&self) -> String {
        format!(
            "{} names parameter {} more than once ({}×)",
            self.definition, self.parameter, self.occurrence_count
        )
    }
}

/// Collects every duplicated lambda-list parameter from every callable
/// definition in one file, with the number of definitions scanned as the
/// denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no repeated parameter here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_duplicate_parameter_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateParameterItem>> {
    let root = tree.root_view();
    let (duplicates, definition_count) = collect_duplicate_parameters(dialect, &root);
    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        duplicates,
        vec![("definition_count", json!(definition_count))],
    ))
}

/// Every duplicated parameter in the top-level definitions of `root`, with the
/// number of definitions scanned.
///
/// Split out of [`build_duplicate_parameter_report`] so the lint rule can reuse
/// the root view the dispatcher already materialized. Rebuilding it from the
/// `SyntaxTree` costs a full extra materialization of the document — a `Vec`
/// per node and a `String` per atom — and the `FileFindings` wrapper the rule
/// would then discard costs a line index over the whole source besides.
#[must_use]
pub fn collect_duplicate_parameters(
    dialect: Dialect,
    root: &ExpressionView,
) -> (Vec<DuplicateParameterItem>, usize) {
    let mut definition_count = 0;
    let mut duplicates = Vec::new();
    if dialect != Dialect::CommonLisp {
        return (duplicates, definition_count);
    }

    for view in &root.children {
        let Some(head) = list_head(view) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, view, head) else {
            continue;
        };
        if !shape.category.is_callable() {
            continue;
        }
        let Some(lambda_list) = shape.lambda_list(view) else {
            continue;
        };
        // A lambda list this parser cannot classify is skipped rather than
        // failing the scan — one unusual macro DSL must not block the rest.
        let Ok(parameter_names) = list_lambda_list_parameter_names(dialect, lambda_list) else {
            continue;
        };
        definition_count += 1;

        let definition = shape.name(view).unwrap_or("?").to_owned();

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
                span: view.span,
                definition: definition.clone(),
                parameter: parameter.clone(),
                occurrence_count: *occurrence_count,
            });
        }
    }

    (duplicates, definition_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateParameterItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_duplicate_parameter_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate parameter report")
    }

    /// The `(definition_count, duplicates)` pair the report is built from.
    fn duplicates(input: &str) -> (u64, Vec<DuplicateParameterItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "definition_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("definition_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(defun f (x x) x)").expect("parse input");
        let report =
            build_duplicate_parameter_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build duplicate parameter report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("definition_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(defun f (x) x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let report = report("(defun g (a) a)\n(defun f (x y x) x)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "duplicate-parameters");
        assert_eq!(
            finding.text_columns(),
            vec![
                "definition=f".to_owned(),
                "parameter=x".to_owned(),
                "count=2".to_owned(),
            ]
        );
        assert_eq!(
            finding.json_fields(),
            vec![
                ("definition", json!("f")),
                ("parameter", json!("x")),
                ("occurrence_count", json!(2)),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_definition_scanned_not_only_the_flagged_ones() {
        let report = report("(defun f (x x) x)\n(defun g (y) y)\n");
        assert_eq!(report.summary, vec![("definition_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
