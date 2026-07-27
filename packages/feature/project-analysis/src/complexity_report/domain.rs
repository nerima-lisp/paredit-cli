//! Per-definition structural complexity metrics for refactor prioritization.
//!
//! Reports nesting depth, atom count, and list count for every
//! definition-like top-level form, plus a composite complexity score, so
//! agents can rank refactor targets without re-deriving structure stats from
//! a raw outline or per-form report themselves.

use std::path::PathBuf;

use anyhow::Result;

use crate::form_report::domain::collect_structural_stats;
use paredit_core_syntax::common_lisp::CommonLispPackageDeclarationForm;
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree,
};

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

/// Nesting depth is weighted more heavily than raw list count: a deeply
/// nested conditional is harder to safely extract than a long but flat
/// body, even when both contain a similar number of forms.
const DEPTH_WEIGHT: usize = 3;

const fn complexity_score(max_depth: usize, list_count: usize) -> usize {
    max_depth
        .saturating_mul(DEPTH_WEIGHT)
        .saturating_add(list_count)
}

#[derive(Debug, Clone)]
pub struct ComplexityReportItem {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub max_depth: usize,
    pub atom_count: usize,
    pub list_count: usize,
    pub complexity_score: usize,
}

#[derive(Debug)]
pub struct ComplexityReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Definitions in this file, sorted by descending complexity score.
    pub definitions: Vec<ComplexityReportItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ComplexityReportPolicyOptions {
    fail_on_max_depth: Option<usize>,
}

impl ComplexityReportPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_max_depth: Option<usize>) -> Self {
        Self { fail_on_max_depth }
    }

    #[must_use]
    pub const fn fail_on_max_depth(self) -> Option<usize> {
        self.fail_on_max_depth
    }
}

#[derive(Debug)]
pub struct ComplexityReportPolicy {
    pub fail_on_max_depth: Option<usize>,
    pub definition_count: usize,
    pub max_depth_overall: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_complexity_report(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<ComplexityReportFile> {
    let mut definitions = Vec::new();

    for index in 0..tree.root_children().len() {
        let form_path = Path::root_child(index);
        let view = tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            continue;
        }
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };

        let (atom_count, list_count, max_depth) = collect_structural_stats(&view);

        definitions.push(ComplexityReportItem {
            path: form_path.to_string(),
            span: view.span,
            head: head.to_owned(),
            name: shape.name(&view).map(ToOwned::to_owned),
            category: shape.category,
            max_depth,
            atom_count,
            list_count,
            complexity_score: complexity_score(max_depth, list_count),
        });
    }

    definitions.sort_by(|left, right| {
        right
            .complexity_score
            .cmp(&left.complexity_score)
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(ComplexityReportFile {
        path,
        dialect,
        definitions,
    })
}

#[must_use]
pub fn evaluate_complexity_report_policy(
    options: ComplexityReportPolicyOptions,
    reports: &[ComplexityReportFile],
) -> ComplexityReportPolicy {
    let definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let max_depth_overall = reports
        .iter()
        .flat_map(|report| &report.definitions)
        .map(|definition| definition.max_depth)
        .max()
        .unwrap_or(0);

    let mut violations = Vec::new();
    if let Some(threshold) = options.fail_on_max_depth() {
        if max_depth_overall > threshold {
            violations.push(format!(
                "max_depth_overall {max_depth_overall} exceeds allowed {threshold}"
            ));
        }
    }

    ComplexityReportPolicy {
        fail_on_max_depth: options.fail_on_max_depth(),
        definition_count,
        max_depth_overall,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn report(input: &str) -> ComplexityReportFile {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_complexity_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build complexity report")
    }

    #[test]
    fn ranks_definitions_by_descending_complexity_score() {
        let report = report(
            "(defun shallow (x) (+ x 1))\n\
             (defun deep (x)\n\
               (if x\n\
                 (let ((y (+ x 1)))\n\
                   (if y (+ y 1) y))\n\
                 x))",
        );

        assert_eq!(report.definitions.len(), 2);
        assert_eq!(report.definitions[0].name.as_deref(), Some("deep"));
        assert_eq!(report.definitions[1].name.as_deref(), Some("shallow"));
        assert!(report.definitions[0].complexity_score > report.definitions[1].complexity_score);
        assert!(report.definitions[0].max_depth > report.definitions[1].max_depth);
    }

    #[test]
    fn ignores_non_definition_top_level_forms() {
        let report = report("(in-package :foo)\n(defun f (x) x)\n(+ 1 2)");

        assert_eq!(report.definitions.len(), 1);
        assert_eq!(report.definitions[0].name.as_deref(), Some("f"));
    }

    #[test]
    fn reports_stable_ordering_for_tied_complexity_scores() {
        let report = report("(defun a (x) x)\n(defun b (x) x)");

        assert_eq!(report.definitions[0].path, "0");
        assert_eq!(report.definitions[1].path, "1");
    }

    #[test]
    fn policy_passes_without_threshold() {
        let report = report("(defun f (x) (if x (if x (if x x x) x) x))");
        let policy = evaluate_complexity_report_policy(
            ComplexityReportPolicyOptions::new(None),
            std::slice::from_ref(&report),
        );

        assert!(policy.passed);
        assert!(policy.violations.is_empty());
    }

    #[test]
    fn policy_fails_when_max_depth_exceeds_threshold() {
        let report = report("(defun f (x) (if x (if x (if x x x) x) x))");
        let max_depth = report.definitions[0].max_depth;
        let policy = evaluate_complexity_report_policy(
            ComplexityReportPolicyOptions::new(Some(max_depth - 1)),
            std::slice::from_ref(&report),
        );

        assert!(!policy.passed);
        assert_eq!(policy.violations.len(), 1);
        assert_eq!(policy.max_depth_overall, max_depth);
    }

    #[test]
    fn policy_passes_when_max_depth_equals_threshold() {
        let report = report("(defun f (x) (if x x x))");
        let max_depth = report.definitions[0].max_depth;
        let policy = evaluate_complexity_report_policy(
            ComplexityReportPolicyOptions::new(Some(max_depth)),
            std::slice::from_ref(&report),
        );

        assert!(policy.passed);
    }
}
