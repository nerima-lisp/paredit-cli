//! Synthesizing a `defpackage` from one file's own definitions and its
//! qualified symbol references.
//!
//! Two questions, answered from what is already in the file rather than from
//! a project-wide symbol table this generator does not have:
//!
//! - **What does this file export?** Every top-level definition whose name
//!   does not start with `%` — the convention this codebase's own reports use
//!   for "internal by name" — is a candidate. A `defmethod` is not: its name
//!   belongs to a `defgeneric` this file may not even define, so exporting it
//!   here would be exporting a symbol from the wrong place half the time.
//! - **What does this file use?** Every `package:symbol` reference in the
//!   file names a package this file depends on. `cl` is assumed and never
//!   listed; anything else becomes a `:use` entry. This is a syntactic scan,
//!   not a package-system resolution — a reference to a package this file
//!   never defines and no other file provides is still listed, the same way
//!   an unresolved `:depends-on` is still listed rather than silently
//!   dropped.
//!
//! Common Lisp only. `defpackage` is a Common Lisp Object System concept; no
//! other dialect this tool parses has one.

use std::collections::{BTreeMap, BTreeSet};

use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};

/// Definition categories a `defpackage` should export. Excludes `Method`
/// (belongs to a generic function, not to this name), `Package` and `System`
/// (not exportable symbols), and `Test` (not part of a public API by
/// convention).
const EXPORTABLE: [DefinitionCategory; 8] = [
    DefinitionCategory::Function,
    DefinitionCategory::Macro,
    DefinitionCategory::GenericFunction,
    DefinitionCategory::Class,
    DefinitionCategory::Struct,
    DefinitionCategory::Condition,
    DefinitionCategory::Variable,
    DefinitionCategory::Constant,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefpackagePlan {
    pub package_name: String,
    pub exports: Vec<String>,
    pub uses: Vec<String>,
    pub generated: String,
    /// The span of an existing `defpackage` this plan replaces, or `None`
    /// when it is a fresh insertion at the top of the file.
    pub replaces: Option<ByteSpan>,
}

#[must_use]
pub fn plan_defpackage(package_name: &str, tree: &SyntaxTree) -> DefpackagePlan {
    let root = tree.root_view();

    let mut exports: BTreeMap<String, String> = BTreeMap::new();
    let mut replaces = None;
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if head.eq_ignore_ascii_case("defpackage") {
            replaces = Some(form.span);
            continue;
        }
        let Some(shape) = definition_shape(Dialect::CommonLisp, form, head) else {
            continue;
        };
        if !EXPORTABLE.contains(&shape.category) {
            continue;
        }
        let Some(name) = shape.name(form) else {
            continue;
        };
        if name.starts_with('%') {
            continue;
        }
        exports
            .entry(name.to_ascii_uppercase())
            .or_insert_with(|| name.to_owned());
    }

    let mut uses: BTreeSet<String> = BTreeSet::new();
    collect_qualified_package_references(&root, &mut uses);
    uses.remove("CL");
    uses.remove("COMMON-LISP");
    uses.remove(&package_name.to_ascii_uppercase());

    let export_names = exports.into_values().collect::<Vec<_>>();
    let use_names = uses
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let generated = render_defpackage(package_name, &export_names, &use_names);

    DefpackagePlan {
        package_name: package_name.to_owned(),
        exports: export_names,
        uses: use_names,
        generated,
        replaces,
    }
}

fn render_defpackage(package_name: &str, exports: &[String], uses: &[String]) -> String {
    let mut use_clause = String::from(":cl");
    for name in uses {
        use_clause.push_str("\n        :");
        use_clause.push_str(name);
    }

    if exports.is_empty() {
        return format!("(defpackage :{package_name}\n  (:use {use_clause}))\n\n");
    }

    let export_clause = exports
        .iter()
        .map(|name| format!("   :{name}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!("(defpackage :{package_name}\n  (:use {use_clause})\n  (:export\n{export_clause}))\n\n")
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

/// The package prefix of a `package:symbol` or `package::symbol` reference,
/// or `None` for a keyword, a string, a reader-macro token, or a bare symbol.
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

    fn plan(source: &str) -> DefpackagePlan {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        plan_defpackage("app", &tree)
    }

    #[test]
    fn a_public_function_is_exported() {
        let found = plan("(defun render (x) x)");
        assert_eq!(found.exports, vec!["render"]);
    }

    #[test]
    fn a_percent_prefixed_name_is_not_exported() {
        let found = plan("(defun %helper () 1)");
        assert!(found.exports.is_empty(), "{found:?}");
    }

    #[test]
    fn a_method_is_not_separately_exported() {
        let found = plan("(defgeneric speak (x))\n(defmethod speak ((x fish)) 1)");
        assert_eq!(found.exports, vec!["speak"]);
    }

    #[test]
    fn a_qualified_reference_becomes_a_use_entry() {
        let found = plan("(defun render () (alexandria:flatten '(1 2)))");
        assert_eq!(found.uses, vec!["alexandria"]);
    }

    #[test]
    fn cl_is_never_listed_as_a_use_entry() {
        let found = plan("(defun render () (cl:+ 1 2))");
        assert!(found.uses.is_empty(), "{found:?}");
    }

    #[test]
    fn a_keyword_is_not_mistaken_for_a_qualified_reference() {
        let found = plan("(defun render () (list :key 1))");
        assert!(found.uses.is_empty(), "{found:?}");
    }

    #[test]
    fn an_existing_defpackage_is_marked_for_replacement() {
        let found = plan("(defpackage :app (:use :cl))\n(defun render () 1)");
        assert!(found.replaces.is_some());
    }

    #[test]
    fn no_existing_defpackage_means_a_fresh_insertion() {
        let found = plan("(defun render () 1)");
        assert!(found.replaces.is_none());
    }

    #[test]
    fn the_generated_form_is_parseable() {
        let found = plan("(defun render (x) x)\n(defparameter *y* 1)");
        SyntaxTree::parse_with_dialect(&found.generated, Dialect::CommonLisp)
            .expect("generated defpackage parses");
    }

    #[test]
    fn duplicate_names_across_definitions_are_exported_once() {
        let found = plan(
            "(defgeneric speak (x))\n(defmethod speak ((x fish)) 1)\n(defmethod speak ((x bird)) 2)",
        );
        assert_eq!(found.exports, vec!["speak"]);
    }
}
