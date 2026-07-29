//! Synthesizing an ASDF `defsystem` from a directory of Common Lisp sources.
//!
//! Two questions, both answered from the files themselves rather than from a
//! system registry this generator does not have:
//!
//! - **What are the components?** One `(:file "stem")` per source file, flat
//!   rather than nested into `:module` components by subdirectory — a
//!   directory-shaped ASDF system is a design decision a generator should not
//!   make silently, so this generates the flat form and leaves nesting to a
//!   caller who wants it.
//! - **What does it depend on?** Every `package:symbol` reference in any
//!   file, minus every package a `defpackage` in the same set of files
//!   defines. What is left is external by elimination: it is used, and
//!   nothing here provides it.
//!
//! Common Lisp only, for the same reason as `generate defpackage`: ASDF is a
//! Common Lisp build system, and ASDF here means `asdf:defsystem`, which no
//! other dialect this tool parses has a use for.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefsystemPlan {
    pub system_name: String,
    pub components: Vec<String>,
    pub depends_on: Vec<String>,
    pub generated: String,
}

#[must_use]
pub fn plan_defsystem(system_name: &str, files: &[(PathBuf, SyntaxTree)]) -> DefsystemPlan {
    let mut components: BTreeSet<String> = BTreeSet::new();
    let mut defined_packages: BTreeSet<String> = BTreeSet::new();
    let mut referenced_packages: BTreeSet<String> = BTreeSet::new();

    for (path, tree) in files {
        if let Some(stem) = component_name(path) {
            components.insert(stem);
        }
        let root = tree.root_view();
        for form in &root.children {
            if let Some(head) = list_head(form) {
                if head.eq_ignore_ascii_case("defpackage") {
                    if let Some(designator) = form.children.get(1).and_then(atom_text) {
                        defined_packages.insert(normalize_designator(designator));
                    }
                }
            }
        }
        collect_qualified_package_references(&root, &mut referenced_packages);
    }

    let mut depends_on = referenced_packages
        .difference(&defined_packages)
        .cloned()
        .collect::<BTreeSet<_>>();
    depends_on.remove("CL");
    depends_on.remove("COMMON-LISP");
    depends_on.remove(&system_name.to_ascii_uppercase());

    let component_names = components.into_iter().collect::<Vec<_>>();
    let depends_on_names = depends_on
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let generated = render_defsystem(system_name, &component_names, &depends_on_names);

    DefsystemPlan {
        system_name: system_name.to_owned(),
        components: component_names,
        depends_on: depends_on_names,
        generated,
    }
}

fn component_name(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned)
}

fn normalize_designator(text: &str) -> String {
    text.trim_start_matches(':')
        .trim_matches('"')
        .to_ascii_uppercase()
}

fn render_defsystem(name: &str, components: &[String], depends_on: &[String]) -> String {
    let depends_clause = if depends_on.is_empty() {
        String::new()
    } else {
        let designators = depends_on
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(" ");
        format!("\n  :depends-on ({designators})")
    };

    if components.is_empty() {
        return format!("(asdf:defsystem \"{name}\"{depends_clause}\n  :components ())\n");
    }

    let components_clause = components
        .iter()
        .map(|name| format!("(:file \"{name}\")"))
        .collect::<Vec<_>>()
        .join("\n               ");

    format!("(asdf:defsystem \"{name}\"{depends_clause}\n  :components ({components_clause}))\n")
}

/// Recursively collects the package prefix of every `package:symbol` atom.
fn collect_qualified_package_references(view: &ExpressionView, found: &mut BTreeSet<String>) {
    if view.kind == ExpressionKind::Atom {
        if let Some(text) = atom_text(view) {
            if let Some(prefix) = qualified_package_prefix(text) {
                found.insert(prefix.to_ascii_uppercase());
            }
        }
    }
    for child in &view.children {
        collect_qualified_package_references(child, found);
    }
}

fn qualified_package_prefix(text: &str) -> Option<&str> {
    if text.starts_with(['"', ':', '#']) {
        return None;
    }
    let colon_index = text.find(':')?;
    if colon_index == 0 {
        return None;
    }
    let prefix = &text[..colon_index];
    (!prefix.is_empty()).then_some(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn parsed(name: &str, source: &str) -> (PathBuf, SyntaxTree) {
        (
            PathBuf::from(name),
            SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse"),
        )
    }

    #[test]
    fn one_file_per_component() {
        let files = [
            parsed("a.lisp", "(defun f () 1)"),
            parsed("b.lisp", "(defun g () 2)"),
        ];
        let plan = plan_defsystem("app", &files);
        assert_eq!(plan.components, vec!["a", "b"]);
    }

    #[test]
    fn an_external_reference_becomes_a_dependency() {
        let files = [parsed("a.lisp", "(defun f () (alexandria:flatten '(1)))")];
        let plan = plan_defsystem("app", &files);
        assert_eq!(plan.depends_on, vec!["alexandria"]);
    }

    #[test]
    fn a_package_defined_by_one_of_the_files_is_not_a_dependency() {
        let files = [
            parsed("pkg.lisp", "(defpackage :internal (:use :cl))"),
            parsed("a.lisp", "(defun f () (internal:helper))"),
        ];
        let plan = plan_defsystem("app", &files);
        assert!(plan.depends_on.is_empty(), "{plan:?}");
    }

    #[test]
    fn cl_is_never_a_dependency() {
        let files = [parsed("a.lisp", "(defun f () (cl:+ 1 2))")];
        let plan = plan_defsystem("app", &files);
        assert!(plan.depends_on.is_empty(), "{plan:?}");
    }

    #[test]
    fn the_generated_form_is_parseable() {
        let files = [parsed("a.lisp", "(defun f () (alexandria:flatten '(1)))")];
        let plan = plan_defsystem("app", &files);
        SyntaxTree::parse(&plan.generated).expect("generated defsystem parses");
    }

    #[test]
    fn no_files_generates_an_empty_component_list() {
        let plan = plan_defsystem("app", &[]);
        assert!(plan.components.is_empty());
        assert_eq!(
            plan.generated,
            "(asdf:defsystem \"app\"\n  :components ())\n"
        );
    }
}
