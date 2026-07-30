//! Whether a file's definitions belong together.
//!
//! A module is a set of definitions that call each other; a namespace is a set
//! that happens to share a file. Nothing in the source distinguishes them, and
//! the difference decides whether splitting a file is a refactor or a
//! rearrangement.
//!
//! The measure is the ratio of *internal* call edges to all call edges. A file
//! whose definitions call each other and little else scores high and is a
//! module. One whose definitions each call outward and never to each other
//! scores zero: nothing binds it together but the filename, and it can be split
//! anywhere.
//!
//! Two definitions are needed before the question means anything. A file with
//! one definition has no internal edges available, and reporting it as zero
//! would rank every small file as maximally incohesive.
//!
//! Scoped to one file, not one package, and the difference matters. A package
//! usually spans files, so a package-level answer needs the whole workspace
//! resolved; this needs only the file in hand, which is what makes it cheap
//! enough to run on everything. The `in-package` in force is reported beside
//! each file so a consumer can aggregate to the package level itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// One definition's coupling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionCoupling {
    pub name: String,
    /// Calls to definitions in this same file.
    pub internal_calls: usize,
    /// Calls to anything else.
    pub external_calls: usize,
    /// Whether nothing in this file calls it *and* it calls nothing in this
    /// file. An isolated definition is the unit a split would move first.
    pub isolated: bool,
    pub span: ByteSpan,
}

impl Finding for DefinitionCoupling {
    fn kind(&self) -> &'static str {
        if self.isolated {
            "isolated"
        } else {
            "connected"
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            format!("internal={}", self.internal_calls),
            format!("external={}", self.external_calls),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("internal_calls", json!(self.internal_calls)),
            ("external_calls", json!(self.external_calls)),
            ("isolated", json!(self.isolated)),
        ]
    }
}

#[must_use]
pub fn build_cohesion_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<DefinitionCoupling> {
    let root = tree.root_view();

    let mut definitions = Vec::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, form, head) else {
            continue;
        };
        let Some(name) = shape.name(form) else {
            continue;
        };
        definitions.push((fold(name), name.to_owned(), form, shape));
    }

    let defined: BTreeSet<String> = definitions.iter().map(|(key, ..)| key.clone()).collect();

    // Who calls whom, so "nothing calls this" can be answered as well as
    // "this calls nothing".
    let mut called_by: BTreeMap<String, usize> = BTreeMap::new();
    let mut edges = Vec::new();
    for (key, _, form, shape) in &definitions {
        let mut internal = 0usize;
        let mut external = 0usize;
        // Heads from the *body*, not from the whole form: `defun` is the head
        // of the definition, not a call the definition makes, and counting it
        // would charge every definition one outward edge it never had.
        let mut heads = Vec::new();
        for body_form in shape.body_forms(form) {
            collect_heads(body_form, &mut heads);
        }
        for head in heads {
            let callee = fold(&head);
            // A self-call is recursion, not cohesion: it says nothing about
            // whether this definition belongs beside the others.
            if &callee == key {
                continue;
            }
            if defined.contains(&callee) {
                internal += 1;
                *called_by.entry(callee).or_default() += 1;
            } else {
                external += 1;
            }
        }
        edges.push((internal, external));
    }

    let findings = definitions
        .iter()
        .zip(&edges)
        .map(
            |((key, name, form, _), (internal, external))| DefinitionCoupling {
                name: name.clone(),
                internal_calls: *internal,
                external_calls: *external,
                isolated: *internal == 0 && !called_by.contains_key(key),
                span: form.span,
            },
        )
        .collect::<Vec<_>>();

    let internal_total: usize = edges.iter().map(|(internal, _)| internal).sum();
    let external_total: usize = edges.iter().map(|(_, external)| external).sum();
    let total = internal_total + external_total;
    // Per mille, so a CI gate compares integers rather than float formatting.
    // A file with fewer than two definitions has no internal edge available,
    // so it is reported as fully cohesive rather than as a total failure.
    let cohesion_per_mille = if definitions.len() < 2 {
        1000
    } else {
        internal_total
            .saturating_mul(1000)
            .checked_div(total)
            .unwrap_or(1000)
    };

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Call heads and definition shapes are dialect-neutral.
        true,
        tree.source(),
        findings,
        vec![
            ("definition_count", json!(definitions.len())),
            ("internal_call_count", json!(internal_total)),
            ("external_call_count", json!(external_total)),
            ("cohesion_per_mille", json!(cohesion_per_mille)),
            ("package", json!(in_package(&root, dialect))),
        ],
    )
}

/// Every list head inside a form, which is every call it makes.
///
/// Syntactic rather than semantic: a head that is a special operator or a macro
/// is counted too. That is the right granularity here, because the question is
/// "do these definitions reference each other", and only names defined in this
/// file are ever counted as internal.
fn collect_heads(view: &ExpressionView, heads: &mut Vec<String>) {
    if let Some(head) = list_head(view) {
        heads.push(head.to_owned());
    }
    for child in &view.children {
        collect_heads(child, heads);
    }
}

/// The package the file's first `in-package` names, when it has one.
///
/// Reported rather than acted on: it is what lets a consumer aggregate these
/// per-file numbers to the package level, which is where the question was
/// originally asked.
fn in_package(root: &ExpressionView, dialect: Dialect) -> Option<String> {
    if dialect != Dialect::CommonLisp {
        return None;
    }
    root.children
        .iter()
        .find(|form| {
            list_head(form).is_some_and(|head| common_lisp_operator_head_eq(head, "in-package"))
        })
        .and_then(|form| form.children.get(1))
        .and_then(atom_symbol_text)
        .map(|name| fold(name.trim_start_matches(['#', ':'])))
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<DefinitionCoupling> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_cohesion_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn summary(report: &FileFindings<DefinitionCoupling>, key: &str) -> Value {
        report
            .summary
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("{key} is reported"))
    }

    #[test]
    fn definitions_that_call_each_other_are_cohesive() {
        let report = report("(defun a () (b))\n(defun b () 1)");
        assert_eq!(summary(&report, "cohesion_per_mille"), json!(1000));
    }

    #[test]
    fn definitions_that_only_call_outward_are_not_cohesive() {
        let report = report("(defun a () (external-one))\n(defun b () (external-two))");
        assert_eq!(summary(&report, "cohesion_per_mille"), json!(0));
    }

    #[test]
    fn a_definition_nothing_links_to_is_reported_as_isolated() {
        let report = report("(defun a () (b))\n(defun b () 1)\n(defun c () (elsewhere))");
        let isolated = report
            .findings
            .iter()
            .filter(|finding| finding.isolated)
            .map(|finding| finding.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(isolated, vec!["c"]);
    }

    #[test]
    fn a_definition_called_by_another_is_not_isolated_even_if_it_calls_nothing() {
        let report = report("(defun a () (b))\n(defun b () 1)");
        assert!(report.findings.iter().all(|finding| !finding.isolated));
    }

    #[test]
    fn recursion_is_not_counted_as_cohesion() {
        let report = report("(defun a () (a))\n(defun b () (elsewhere))");
        assert_eq!(summary(&report, "internal_call_count"), json!(0));
    }

    #[test]
    fn a_single_definition_file_is_not_reported_as_a_total_failure() {
        let report = report("(defun a () (elsewhere))");
        assert_eq!(summary(&report, "cohesion_per_mille"), json!(1000));
    }

    #[test]
    fn an_empty_file_measures_fully_cohesive_rather_than_dividing_by_zero() {
        let report = report("");
        assert_eq!(summary(&report, "cohesion_per_mille"), json!(1000));
    }

    #[test]
    fn the_package_in_force_is_reported_for_aggregation() {
        let report = report("(in-package :app)\n(defun a () 1)");
        assert_eq!(summary(&report, "package"), json!("APP"));
    }

    #[test]
    fn a_file_with_no_in_package_reports_none() {
        let report = report("(defun a () 1)");
        assert_eq!(summary(&report, "package"), Value::Null);
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun a () 1)\n(defun b () 2)\n(defun c () 3)");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }
}
