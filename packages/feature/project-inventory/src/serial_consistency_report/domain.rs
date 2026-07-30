//! Whether a `:serial t` claim matches the dependencies the components declare.
//!
//! `:serial t` tells ASDF that every component depends on the one before it.
//! That is a strong claim, it is almost always written once and never revisited,
//! and it goes wrong in two directions that look nothing alike:
//!
//! - A component under `:serial t` that *also* declares `:depends-on` is
//!   **redundant** when it names an earlier sibling — ASDF already knows — and
//!   **contradictory** when it names a *later* one, which is a dependency the
//!   serial order cannot satisfy and a load failure waiting for a clean image.
//! - A system with no `:serial t` and no `:depends-on` anywhere has an
//!   **unordered** component list: ASDF may load in any order, and the file
//!   order everyone assumes is not a guarantee.
//!
//! Reordering a `:serial t` list is the operation this makes safe. Without it,
//! moving a component is a change whose blast radius is invisible.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// What is inconsistent about one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialFault {
    /// A `:depends-on` on an earlier sibling that `:serial t` already implies.
    Redundant,
    /// A `:depends-on` on a *later* sibling, which the serial order cannot
    /// satisfy.
    Contradictory,
    /// The system is not serial and this component declares no dependencies,
    /// so its load position is not guaranteed.
    Unordered,
    /// Consistent. Reported so the denominator is visible.
    Consistent,
}

impl SerialFault {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Redundant => "redundant-dependency",
            Self::Contradictory => "contradictory-dependency",
            Self::Unordered => "unordered-component",
            Self::Consistent => "consistent",
        }
    }

    #[must_use]
    pub const fn is_fault(self) -> bool {
        !matches!(self, Self::Consistent)
    }
}

/// One component of one system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentFinding {
    pub fault: SerialFault,
    pub system: String,
    pub component: String,
    /// Its position in the component list, 0-based.
    pub position: usize,
    /// Whether the enclosing system declares `:serial t`.
    pub serial: bool,
    /// The declared dependencies that caused the fault.
    pub dependencies: Vec<String>,
    pub span: ByteSpan,
}

impl Finding for ComponentFinding {
    fn kind(&self) -> &'static str {
        self.fault.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.system.clone(),
            self.component.clone(),
            format!("position={}", self.position),
            format!("serial={}", self.serial),
            format!("depends=({})", self.dependencies.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("fault", json!(self.fault.label())),
            ("system", json!(self.system)),
            ("component", json!(self.component)),
            ("position", json!(self.position)),
            ("serial", json!(self.serial)),
            ("dependencies", json!(self.dependencies)),
        ]
    }
}

#[must_use]
pub fn build_serial_consistency_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<ComponentFinding> {
    let modelled = dialect == Dialect::CommonLisp;
    let mut findings = Vec::new();

    if modelled {
        for form in &tree.root_view().children {
            let Some(head) = list_head(form) else {
                continue;
            };
            if !common_lisp_operator_head_eq(head, "defsystem") {
                continue;
            }
            let Some(system) = form
                .children
                .get(1)
                .and_then(atom_symbol_text)
                .map(designator)
            else {
                continue;
            };
            let serial = is_serial(form);
            let Some(components) = option_list(form, ":components") else {
                continue;
            };

            let positions: BTreeMap<String, usize> = components
                .iter()
                .enumerate()
                .filter_map(|(index, component)| Some((component_name(component)?, index)))
                .collect();

            for (index, component) in components.iter().enumerate() {
                let Some(name) = component_name(component) else {
                    continue;
                };
                let dependencies = component_dependencies(component);
                let fault = classify(serial, index, &dependencies, &positions);
                findings.push(ComponentFinding {
                    fault,
                    system: system.clone(),
                    component: name,
                    position: index,
                    serial,
                    dependencies,
                    span: component.span,
                });
            }
        }
    }

    let faults = findings
        .iter()
        .filter(|finding: &&ComponentFinding| finding.fault.is_fault())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        vec![("fault_count", json!(faults))],
    )
}

/// The first inconsistency, in severity order.
///
/// A contradiction outranks redundancy: one fails a clean load and the other is
/// merely noise, and a component with both should report the one that breaks.
fn classify(
    serial: bool,
    index: usize,
    dependencies: &[String],
    positions: &BTreeMap<String, usize>,
) -> SerialFault {
    if !serial {
        return if dependencies.is_empty() && positions.len() > 1 {
            SerialFault::Unordered
        } else {
            SerialFault::Consistent
        };
    }
    if dependencies
        .iter()
        .filter_map(|name| positions.get(name))
        .any(|position| *position > index)
    {
        return SerialFault::Contradictory;
    }
    if dependencies
        .iter()
        .filter_map(|name| positions.get(name))
        .any(|position| *position < index)
    {
        return SerialFault::Redundant;
    }
    SerialFault::Consistent
}

fn is_serial(form: &ExpressionView) -> bool {
    let mut children = form.children.iter().skip(2);
    while let Some(child) = children.next() {
        if atom_symbol_text(child).is_some_and(|text| text.eq_ignore_ascii_case(":serial")) {
            return children
                .next()
                .and_then(atom_symbol_text)
                .is_some_and(|text| text.eq_ignore_ascii_case("t"));
        }
    }
    false
}

fn option_list<'a>(form: &'a ExpressionView, keyword: &str) -> Option<&'a [ExpressionView]> {
    let mut children = form.children.iter().skip(2);
    while let Some(child) = children.next() {
        if atom_symbol_text(child).is_some_and(|text| text.eq_ignore_ascii_case(keyword)) {
            return children.next().map(|list| list.children.as_slice());
        }
    }
    None
}

/// A component's name: `(:file "core")` names `core`.
fn component_name(component: &ExpressionView) -> Option<String> {
    component
        .children
        .get(1)
        .and_then(atom_symbol_text)
        .map(designator)
}

fn component_dependencies(component: &ExpressionView) -> Vec<String> {
    option_list_at(component, ":depends-on")
        .map(|entries| {
            entries
                .iter()
                .filter_map(atom_symbol_text)
                .map(designator)
                .collect()
        })
        .unwrap_or_default()
}

/// The same option scan, from a component's own children rather than a
/// system's. A component's name occupies index 1, so its options start at 2.
fn option_list_at<'a>(
    component: &'a ExpressionView,
    keyword: &str,
) -> Option<&'a [ExpressionView]> {
    let mut children = component.children.iter().skip(2);
    while let Some(child) = children.next() {
        if atom_symbol_text(child).is_some_and(|text| text.eq_ignore_ascii_case(keyword)) {
            return children.next().map(|list| list.children.as_slice());
        }
    }
    None
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

    fn report(source: &str) -> FileFindings<ComponentFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_serial_consistency_report(Path::new("t.asd"), Dialect::CommonLisp, &tree)
    }

    fn faults(source: &str) -> Vec<SerialFault> {
        report(source).findings.iter().map(|f| f.fault).collect()
    }

    #[test]
    fn a_plain_serial_system_is_consistent() {
        assert_eq!(
            faults("(defsystem \"app\" :serial t :components ((:file \"a\") (:file \"b\")))"),
            vec![SerialFault::Consistent, SerialFault::Consistent]
        );
    }

    #[test]
    fn a_dependency_on_an_earlier_sibling_under_serial_is_redundant() {
        assert_eq!(
            faults(
                "(defsystem \"app\" :serial t :components \
                 ((:file \"a\") (:file \"b\" :depends-on (\"a\"))))"
            ),
            vec![SerialFault::Consistent, SerialFault::Redundant]
        );
    }

    #[test]
    fn a_dependency_on_a_later_sibling_under_serial_is_a_contradiction() {
        assert_eq!(
            faults(
                "(defsystem \"app\" :serial t :components \
                 ((:file \"a\" :depends-on (\"b\")) (:file \"b\")))"
            ),
            vec![SerialFault::Contradictory, SerialFault::Consistent]
        );
    }

    #[test]
    fn a_non_serial_system_with_no_dependencies_is_unordered() {
        assert_eq!(
            faults("(defsystem \"app\" :components ((:file \"a\") (:file \"b\")))"),
            vec![SerialFault::Unordered, SerialFault::Unordered]
        );
    }

    #[test]
    fn a_non_serial_system_with_explicit_dependencies_is_consistent() {
        assert_eq!(
            faults(
                "(defsystem \"app\" :components \
                 ((:file \"a\") (:file \"b\" :depends-on (\"a\"))))"
            ),
            vec![SerialFault::Unordered, SerialFault::Consistent]
        );
    }

    #[test]
    fn a_serial_false_is_not_read_as_serial() {
        assert_eq!(
            faults("(defsystem \"app\" :serial nil :components ((:file \"a\") (:file \"b\")))"),
            vec![SerialFault::Unordered, SerialFault::Unordered]
        );
    }

    #[test]
    fn a_dependency_on_something_outside_the_component_list_is_not_a_fault() {
        assert_eq!(
            faults(
                "(defsystem \"app\" :serial t :components \
                 ((:file \"a\" :depends-on (\"alexandria\"))))"
            ),
            vec![SerialFault::Consistent]
        );
    }

    #[test]
    fn a_system_with_no_components_reports_nothing() {
        assert!(report("(defsystem \"app\" :serial t)").findings.is_empty());
    }

    #[test]
    fn the_fault_count_excludes_consistent_components() {
        let report = report(
            "(defsystem \"app\" :serial t :components \
             ((:file \"a\") (:file \"b\" :depends-on (\"a\"))))",
        );
        assert_eq!(report.summary, vec![("fault_count", json!(1))]);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defproject app)", Dialect::Clojure).expect("parse");
        let report = build_serial_consistency_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
