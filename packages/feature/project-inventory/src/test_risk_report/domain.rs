use std::path::{Path, PathBuf};

use paredit_core_syntax::definition::DefinitionCategory;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::LineIndex;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_feature_project_analysis::complexity_report::domain::{
    ComplexityReportItem, build_complexity_report,
};
use paredit_feature_project_analysis::error::ProjectAnalysisResult;

use crate::test_map_report::domain::{Coverage, build_test_map_reports};

pub const COVERAGE_BASIS: &str = "static-test-name-linkage";
pub const LIMITATION: &str = "Naming conventions infer test links, not execution coverage or safety. Only named top-level Common Lisp and Emacs Lisp functions are modelled.";

#[derive(Debug)]
pub struct TestRiskFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub modelled: bool,
    pub function_count: Option<usize>,
    pub linked_function_count: Option<usize>,
    pub unlinked_function_count: Option<usize>,
}

#[derive(Debug)]
pub struct TestRiskEntry {
    pub file: PathBuf,
    pub dialect: Dialect,
    pub name: String,
    pub line: usize,
    pub complexity: ComplexityReportItem,
}

#[derive(Debug)]
pub struct TestRiskReport {
    pub files: Vec<TestRiskFile>,
    pub ranked: Vec<TestRiskEntry>,
}

pub fn build_test_risk_report(
    files: &[(&Path, Dialect, &SyntaxTree)],
) -> ProjectAnalysisResult<TestRiskReport> {
    let mut files = files.to_vec();
    files.sort_by(|left, right| left.0.cmp(right.0));
    let supported = files
        .iter()
        .copied()
        .filter(|(_, dialect, _)| matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp))
        .collect::<Vec<_>>();
    // Keep the fileset-wide package identity and ambiguous-name refusal from test-map.
    let maps = build_test_map_reports(&supported);
    let mut reports = Vec::with_capacity(files.len());
    let mut ranked = Vec::new();
    for (path, dialect, _) in &files {
        if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
            reports.push(TestRiskFile {
                path: path.to_path_buf(),
                dialect: *dialect,
                modelled: false,
                function_count: None,
                linked_function_count: None,
                unlinked_function_count: None,
            });
        }
    }
    for ((path, dialect, tree), map) in supported.iter().zip(maps) {
        let complexity = build_complexity_report(path.to_path_buf(), *dialect, tree)?;
        let lines = LineIndex::new(tree.source());
        let mut function_count = 0;
        let mut linked_function_count = 0;
        for item in complexity.definitions {
            if item.category != DefinitionCategory::Function {
                continue;
            }
            let Some(link) = map.findings.iter().find(|entry| entry.span == item.span) else {
                // Both inventories exclude malformed definitions without a readable name.
                continue;
            };
            function_count += 1;
            if link.coverage == Coverage::Tested {
                linked_function_count += 1;
            } else {
                ranked.push(TestRiskEntry {
                    file: path.to_path_buf(),
                    dialect: *dialect,
                    name: link.name.clone(),
                    line: lines.line_of(item.span.start().get()),
                    complexity: item,
                });
            }
        }
        reports.push(TestRiskFile {
            path: path.to_path_buf(),
            dialect: *dialect,
            modelled: true,
            function_count: Some(function_count),
            linked_function_count: Some(linked_function_count),
            unlinked_function_count: Some(function_count - linked_function_count),
        });
    }
    reports.sort_by(|left, right| left.path.cmp(&right.path));
    ranked.sort_by(|left, right| {
        right
            .complexity
            .complexity_score
            .cmp(&left.complexity.complexity_score)
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| {
                left.complexity
                    .span
                    .start()
                    .cmp(&right.complexity.span.start())
            })
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(TestRiskReport {
        files: reports,
        ranked,
    })
}

#[cfg(test)]
mod tests;
