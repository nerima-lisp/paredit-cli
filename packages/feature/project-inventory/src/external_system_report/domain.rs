//! The ASDF systems this project depends on but does not define.
//!
//! An SBOM, in effect, and the input to every question that starts "what would
//! break if". `inspect dependencies` reports the edges; this reports the
//! *boundary* — which of those edges leave the project — which is the half a
//! supply-chain question needs and the half an edge list buries.
//!
//! A dependency the analyzed files also define is *internal* and reported as
//! such rather than dropped, so the output is a complete account of the
//! `:depends-on` graph rather than a filtered one.
//!
//! Both `:depends-on` spellings are read: a bare designator, and the
//! `(:version "x" "1.0")` / `(:feature :sbcl "y")` forms, whose system name is
//! not in the first position and which a naive reader mistakes for a feature
//! expression.

use std::collections::BTreeSet;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// One `:depends-on` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDependency {
    pub name: String,
    /// The system that depends on it.
    pub dependent: String,
    /// Whether the analyzed files also define it.
    pub internal: bool,
    /// A version floor from `(:version "x" "1.0")`, when one is declared.
    pub version: Option<String>,
    /// The feature a `(:feature :sbcl "y")` dependency is conditional on.
    pub feature: Option<String>,
    pub span: ByteSpan,
}

impl Finding for SystemDependency {
    fn kind(&self) -> &'static str {
        if self.internal {
            "internal"
        } else {
            "external"
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.dependent.clone(),
            self.name.clone(),
            self.version.clone().unwrap_or_else(|| "-".to_owned()),
            self.feature.clone().unwrap_or_else(|| "-".to_owned()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("dependent", json!(self.dependent)),
            ("internal", json!(self.internal)),
            ("version", json!(self.version)),
            ("feature", json!(self.feature)),
        ]
    }
}

#[must_use]
pub fn build_external_system_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<SystemDependency> {
    let modelled = dialect == Dialect::CommonLisp;
    let root = tree.root_view();

    let systems: BTreeSet<String> = if modelled {
        root.children
            .iter()
            .filter_map(|form| system_name(form).map(|(name, _)| name))
            .collect()
    } else {
        BTreeSet::new()
    };

    let mut findings = Vec::new();
    if modelled {
        for form in &root.children {
            let Some((dependent, _)) = system_name(form) else {
                continue;
            };
            // `defsystem` options are flat keyword/value pairs — `:depends-on
            // (a b)` — not `defpackage`-style `(:depends-on a b)` sublists.
            // Reading them the other way finds nothing at all.
            for entry in depends_on(form) {
                let Some((name, version, feature)) = dependency(entry) else {
                    continue;
                };
                findings.push(SystemDependency {
                    internal: systems.contains(&name),
                    name,
                    dependent: dependent.clone(),
                    version,
                    feature,
                    span: entry.span,
                });
            }
        }
    }

    let external = findings
        .iter()
        .filter(|dependency: &&SystemDependency| !dependency.internal)
        .map(|dependency| dependency.name.clone())
        .collect::<BTreeSet<_>>()
        .len();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        vec![
            ("system_count", json!(systems.len())),
            ("distinct_external_count", json!(external)),
        ],
    )
}

/// The entries of a `defsystem`'s `:depends-on` option.
fn depends_on(form: &ExpressionView) -> &[ExpressionView] {
    let mut children = form.children.iter().skip(2);
    while let Some(child) = children.next() {
        if atom_symbol_text(child).is_some_and(|text| text.eq_ignore_ascii_case(":depends-on")) {
            return children
                .next()
                .map_or(&[][..], |list| list.children.as_slice());
        }
    }
    &[]
}

/// The name of the system a `defsystem` form defines.
fn system_name(form: &ExpressionView) -> Option<(String, ByteSpan)> {
    let head = list_head(form)?;
    if !common_lisp_operator_head_eq(head, "defsystem") {
        return None;
    }
    let name = form.children.get(1)?;
    Some((designator(atom_symbol_text(name)?), name.span))
}

/// Reads one `:depends-on` entry: a bare designator, or one of ASDF's
/// wrapped forms.
///
/// The wrapped forms put the system name in third position, not first, so a
/// reader that takes the first element gets `:version` as a system name.
fn dependency(entry: &ExpressionView) -> Option<(String, Option<String>, Option<String>)> {
    match entry.kind {
        ExpressionKind::Atom => Some((designator(atom_symbol_text(entry)?), None, None)),
        ExpressionKind::List => {
            let head = entry.children.first().and_then(atom_symbol_text)?;
            let name = designator(atom_symbol_text(entry.children.get(1)?)?);
            if head.eq_ignore_ascii_case(":version") {
                let version = entry
                    .children
                    .get(2)
                    .and_then(atom_symbol_text)
                    .map(|text| text.trim_matches('"').to_owned());
                Some((name, version, None))
            } else if head.eq_ignore_ascii_case(":feature") {
                // `(:feature :sbcl "system")`: the *second* element is the
                // feature and the third is the system.
                let system = entry
                    .children
                    .get(2)
                    .and_then(atom_symbol_text)
                    .map(designator)?;
                Some((system, None, Some(name)))
            } else if head.eq_ignore_ascii_case(":require") {
                Some((name, None, None))
            } else {
                None
            }
        }
        ExpressionKind::Root => None,
    }
}

fn designator(name: &str) -> String {
    name.trim_start_matches("#:")
        .trim_start_matches(':')
        .trim_matches(['|', '"'])
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<SystemDependency> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_external_system_report(Path::new("t.asd"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_bare_dependency_is_external_when_nothing_defines_it() {
        let report = report("(defsystem \"app\" :depends-on (\"alexandria\"))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].name, "alexandria");
        assert!(!report.findings[0].internal);
    }

    #[test]
    fn a_dependency_the_file_also_defines_is_internal() {
        let report =
            report("(defsystem \"app\" :depends-on (\"app/core\"))\n(defsystem \"app/core\")");
        assert!(report.findings[0].internal);
        assert_eq!(report.summary[1], ("distinct_external_count", json!(0)));
    }

    #[test]
    fn a_version_floor_is_read_rather_than_mistaken_for_a_system() {
        let report = report("(defsystem \"app\" :depends-on ((:version \"alexandria\" \"1.0\")))");
        assert_eq!(report.findings[0].name, "alexandria");
        assert_eq!(report.findings[0].version.as_deref(), Some("1.0"));
    }

    #[test]
    fn a_feature_conditional_dependency_names_the_system_not_the_feature() {
        let report = report("(defsystem \"app\" :depends-on ((:feature :sbcl \"sb-posix\")))");
        assert_eq!(report.findings[0].name, "sb-posix");
        assert_eq!(report.findings[0].feature.as_deref(), Some("sbcl"));
    }

    #[test]
    fn every_designator_spelling_names_the_same_system() {
        for spelling in [
            "\"alexandria\"",
            "#:alexandria",
            ":alexandria",
            "alexandria",
        ] {
            let report = report(&format!("(defsystem \"app\" :depends-on ({spelling}))"));
            assert_eq!(report.findings[0].name, "alexandria", "{spelling}");
        }
    }

    #[test]
    fn a_distinct_count_does_not_double_count_one_system() {
        let report =
            report("(defsystem \"a\" :depends-on (\"x\"))\n(defsystem \"b\" :depends-on (\"x\"))");
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.summary[1], ("distinct_external_count", json!(1)));
    }

    #[test]
    fn a_file_with_no_defsystem_reports_nothing() {
        let report = report("(defun f () 1)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defproject app)", Dialect::Clojure).expect("parse");
        let report = build_external_system_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
