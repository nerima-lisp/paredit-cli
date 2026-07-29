//! What a package promises: every exported symbol, with the signature the
//! export commits to.
//!
//! `defpackage`'s `:export` list is a list of names. What a caller can actually
//! rely on is those names *plus their shapes* — a function's arity, a class's
//! slots, a constant's value — and that pairing exists nowhere in the source.
//! Producing it is what makes a release check possible: `api-diff` compares two
//! of these snapshots, and it can only do so because the snapshot carries more
//! than names.
//!
//! An exported symbol with no definition in the analyzed files is reported as
//! *undefined* rather than dropped. That is usually the actual defect — a
//! rename that updated the definition and not the `:export`, which fails at
//! load time on a fresh image and never on a warm one.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// One exported symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiEntry {
    /// The exported name, reader-folded.
    pub name: String,
    /// The package that exports it.
    pub package: String,
    /// The defining head, when the analyzed files define it.
    pub category: Option<String>,
    /// Required and maximum argument counts, where the definition has a lambda
    /// list. `None` for a maximum means `&rest` or `&key`: unbounded.
    pub required_arity: Option<usize>,
    pub max_arity: Option<usize>,
    /// The lambda list as written, which is the part a caller reads.
    pub lambda_list: Option<String>,
    /// Whether the analyzed files define this export at all.
    pub defined: bool,
    pub span: ByteSpan,
    pub line: usize,
}

impl ApiEntry {
    /// The stable identity of this entry, for diffing two snapshots.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.package, self.name)
    }

    /// The shape a caller depends on, as one comparable string.
    #[must_use]
    pub fn signature(&self) -> String {
        format!(
            "{}/{}..{}",
            self.category.as_deref().unwrap_or("?"),
            self.required_arity
                .map_or_else(|| "?".to_owned(), |arity| arity.to_string()),
            self.max_arity
                .map_or_else(|| "*".to_owned(), |arity| arity.to_string()),
        )
    }
}

impl Finding for ApiEntry {
    fn kind(&self) -> &'static str {
        if self.defined {
            "export"
        } else {
            "undefined-export"
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.package.clone(),
            self.name.clone(),
            self.signature(),
            self.lambda_list.clone().unwrap_or_else(|| "-".to_owned()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("package", json!(self.package)),
            ("category", json!(self.category)),
            ("required_arity", json!(self.required_arity)),
            ("max_arity", json!(self.max_arity)),
            ("lambda_list", json!(self.lambda_list)),
            ("defined", json!(self.defined)),
            ("signature", json!(self.signature())),
        ]
    }
}

/// What the analyzed file knows about one definition.
struct Definition {
    category: String,
    required: Option<usize>,
    max: Option<usize>,
    lambda_list: Option<String>,
    span: ByteSpan,
}

#[must_use]
pub fn build_api_surface_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<ApiEntry> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    if !modelled {
        return FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("undefined_export_count", json!(0))],
        );
    }

    let root = tree.root_view();
    let definitions = collect_definitions(&root, dialect, source);

    let mut findings = Vec::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !common_lisp_operator_head_eq(head, "defpackage") {
            continue;
        }
        let Some(package) = form
            .children
            .get(1)
            .and_then(atom_symbol_text)
            .map(designator)
        else {
            continue;
        };
        for option in form.children.iter().skip(2) {
            let Some(option_head) = option.children.first().and_then(atom_symbol_text) else {
                continue;
            };
            if !option_head.eq_ignore_ascii_case(":export") {
                continue;
            }
            for exported in option.children.iter().skip(1) {
                let Some(name) = atom_symbol_text(exported).map(designator) else {
                    continue;
                };
                let definition = definitions.get(&name);
                findings.push(ApiEntry {
                    category: definition.map(|found| found.category.clone()),
                    required_arity: definition.and_then(|found| found.required),
                    max_arity: definition.and_then(|found| found.max),
                    lambda_list: definition.and_then(|found| found.lambda_list.clone()),
                    defined: definition.is_some(),
                    // The finding points at the definition when there is one —
                    // that is where a reader needs to go — and at the `:export`
                    // designator when there is not, since that is the only
                    // thing that exists.
                    span: definition.map_or(exported.span, |found| found.span),
                    line: line_of(
                        source,
                        definition
                            .map_or(exported.span, |found| found.span)
                            .start()
                            .get(),
                    ),
                    package: package.clone(),
                    name,
                });
            }
        }
    }

    let undefined = findings.iter().filter(|entry| !entry.defined).count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        findings,
        vec![("undefined_export_count", json!(undefined))],
    )
}

fn collect_definitions(
    root: &ExpressionView,
    dialect: Dialect,
    source: &str,
) -> BTreeMap<String, Definition> {
    let mut definitions = BTreeMap::new();
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
        let (required, max) = shape
            .lambda_parameter_arity(form)
            .map_or((None, None), |(required, max)| (Some(required), max));
        let lambda_list = shape.lambda_list(form).map(|list| {
            source
                .get(list.span.start().get()..list.span.end().get())
                .unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        });
        // First definition wins, matching how a loader sees a redefinition:
        // whichever ran first is what a caller compiled against.
        definitions.entry(designator(name)).or_insert(Definition {
            category: head.to_ascii_lowercase(),
            required,
            max,
            lambda_list,
            span: form.span,
        });
    }
    definitions
}

/// A symbol designator as the reader folds it: `#:foo`, `:foo`, `|foo|`, and
/// `foo` all name the same symbol.
fn designator(name: &str) -> String {
    name.trim_start_matches("#:")
        .trim_start_matches(':')
        .trim_matches('|')
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<ApiEntry> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_api_surface_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn an_exported_function_carries_its_arity() {
        let report = report("(defpackage :app (:export #:render))\n(defun render (pane x) x)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        let entry = &report.findings[0];
        assert_eq!(entry.name, "RENDER");
        assert_eq!(entry.package, "APP");
        assert_eq!(entry.required_arity, Some(2));
        assert_eq!(entry.max_arity, Some(2));
        assert!(entry.defined);
    }

    #[test]
    fn an_optional_parameter_widens_the_maximum_without_moving_the_minimum() {
        let report =
            report("(defpackage :app (:export #:f))\n(defun f (a &optional b) (list a b))");
        assert_eq!(report.findings[0].required_arity, Some(1));
        assert_eq!(report.findings[0].max_arity, Some(2));
    }

    #[test]
    fn a_rest_parameter_makes_the_maximum_unbounded() {
        let report =
            report("(defpackage :app (:export #:f))\n(defun f (a &rest more) (list a more))");
        assert_eq!(report.findings[0].max_arity, None);
        assert!(report.findings[0].signature().ends_with("..*"));
    }

    #[test]
    fn an_export_nothing_defines_is_reported_rather_than_dropped() {
        let report = report("(defpackage :app (:export #:renderr))\n(defun render (x) x)");
        assert_eq!(report.findings[0].kind(), "undefined-export");
        assert!(!report.findings[0].defined);
        assert_eq!(report.summary, vec![("undefined_export_count", json!(1))]);
    }

    #[test]
    fn a_definition_that_is_not_exported_is_not_part_of_the_surface() {
        let report = report("(defpackage :app (:export #:a))\n(defun a () 1)\n(defun b () 2)");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].name, "A");
    }

    #[test]
    fn every_designator_spelling_names_the_same_symbol() {
        for spelling in ["#:render", ":render", "render", "|render|"] {
            let report = report(&format!(
                "(defpackage :app (:export {spelling}))\n(defun render (x) x)"
            ));
            assert!(report.findings[0].defined, "{spelling}");
        }
    }

    #[test]
    fn the_signature_is_one_comparable_string() {
        let report = report("(defpackage :app (:export #:f))\n(defun f (a) a)");
        assert_eq!(report.findings[0].signature(), "defun/1..1");
    }

    #[test]
    fn the_key_is_package_qualified_so_two_packages_do_not_collide() {
        let report =
            report("(defpackage :a (:export #:f))\n(defpackage :b (:export #:f))\n(defun f () 1)");
        let keys = report
            .findings
            .iter()
            .map(ApiEntry::key)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["A:F".to_owned(), "B:F".to_owned()]);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree = SyntaxTree::parse_with_dialect("(ns app)", Dialect::Clojure).expect("parse");
        let report = build_api_surface_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
