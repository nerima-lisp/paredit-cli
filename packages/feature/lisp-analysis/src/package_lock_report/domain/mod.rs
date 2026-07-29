//! Definitions and bindings that collide with a symbol in the `COMMON-LISP`
//! package.
//!
//! CLHS 11.1.2.1.2 lists the consequences of redefining, binding, or declaring
//! a standard symbol as *undefined*. Implementations diverge accordingly: SBCL
//! and CCL signal a package-lock violation and refuse; others accept it and
//! produce a program whose meaning depends on load order.
//!
//! The failure mode is what makes this worth a report rather than a lint rule.
//! `(defun list (…) …)` is perfectly well-formed, breaks nothing at read time,
//! and detonates on the *next machine* — a portability bug that a test suite on
//! one implementation cannot find.
//!
//! Severity is split, because the three ways to collide with a standard symbol
//! are not equally serious:
//!
//! - **Redefinition** — `defun`, `defmacro`, `defclass` on a standard name.
//!   Undefined consequences, and locked implementations refuse to load.
//! - **Binding** — a `let` or lambda list naming `list` or `t`. Legal for a
//!   non-special standard symbol, and still a trap: the body's `(list …)` calls
//!   are unaffected, so the shadow is invisible where it matters most.
//! - **Shadowing** — `(:shadow #:list)` in a `defpackage`. Explicit, deliberate,
//!   and reported only so the inventory is complete.

use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

mod standard_symbols;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
pub use standard_symbols::is_standard_symbol;

/// How a name collides with the `COMMON-LISP` package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// A top-level definition of a standard name.
    Redefinition,
    /// A lexical binding of a standard name.
    Binding,
    /// An explicit `(:shadow …)` in a `defpackage`.
    Shadow,
}

impl Collision {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Redefinition => "redefinition",
            Self::Binding => "binding",
            Self::Shadow => "shadow",
        }
    }

    /// Whether CLHS leaves the consequences undefined.
    ///
    /// A declared shadow is the one form of this that the standard blesses:
    /// `defpackage` exists to say "I mean my own `list`, not yours".
    #[must_use]
    pub const fn is_undefined_behavior(self) -> bool {
        matches!(self, Self::Redefinition | Self::Binding)
    }
}

/// One collision with a standard symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLock {
    pub collision: Collision,
    pub name: String,
    /// The form that caused it (`defun`, `let`, `defpackage`, …).
    pub form: String,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for PackageLock {
    fn kind(&self) -> &'static str {
        self.collision.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.name.clone(), self.form.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("collision", json!(self.collision.label())),
            ("name", json!(self.name)),
            ("form", json!(self.form)),
        ]
    }
}

/// Binding forms whose second child is a `((name value) …)` binding list.
const BINDING_FORMS: [&str; 6] = [
    "let",
    "let*",
    "flet",
    "labels",
    "macrolet",
    "symbol-macrolet",
];

#[must_use]
pub fn build_package_lock_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<PackageLock> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut findings = Vec::new();

    if modelled {
        collect(&tree.root_view(), dialect, source, &mut findings);
    }

    let undefined = findings
        .iter()
        .filter(|finding: &&PackageLock| finding.collision.is_undefined_behavior())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![("undefined_behavior_count", json!(undefined))],
    )
}

fn collect(view: &ExpressionView, dialect: Dialect, source: &str, findings: &mut Vec<PackageLock>) {
    if let Some(head) = list_head(view) {
        // A definition of a standard name.
        if let Some(shape) = definition_shape(dialect, view, head) {
            if let Some(name) = shape.name(view) {
                push(
                    findings,
                    Collision::Redefinition,
                    name,
                    head,
                    view.span,
                    source,
                );
            }
        }

        if BINDING_FORMS
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            for binding in view
                .children
                .get(1)
                .map(|list| list.children.as_slice())
                .unwrap_or_default()
            {
                if let Some(name) = binding_name(binding) {
                    push(
                        findings,
                        Collision::Binding,
                        name,
                        head,
                        binding.span,
                        source,
                    );
                }
            }
        }

        if common_lisp_operator_head_eq(head, "defpackage") {
            for option in view.children.iter().skip(2) {
                let Some(option_head) = option.children.first().and_then(atom_symbol_text) else {
                    continue;
                };
                if !option_head.eq_ignore_ascii_case(":shadow") {
                    continue;
                }
                for designator in option.children.iter().skip(1) {
                    if let Some(name) = atom_symbol_text(designator) {
                        push(
                            findings,
                            Collision::Shadow,
                            name,
                            head,
                            designator.span,
                            source,
                        );
                    }
                }
            }
        }
    }

    for child in &view.children {
        collect(child, dialect, source, findings);
    }
}

/// Records a finding when the name is a `COMMON-LISP` symbol.
///
/// The designator is normalized first: `#:list`, `:list`, and `|list|` all name
/// the same symbol, and a table lookup on the raw spelling would miss two of
/// the three.
fn push(
    findings: &mut Vec<PackageLock>,
    collision: Collision,
    name: &str,
    form: &str,
    span: ByteSpan,
    source: &str,
) {
    let normalized = normalize(name);
    if !is_standard_symbol(&normalized) {
        return;
    }
    findings.push(PackageLock {
        collision,
        name: normalized,
        form: form.to_owned(),
        span,
        line: line_of(source, span.start().get()),
    });
}

fn binding_name(binding: &ExpressionView) -> Option<&str> {
    match binding.kind {
        ExpressionKind::Atom => atom_symbol_text(binding),
        ExpressionKind::List => binding.children.first().and_then(atom_symbol_text),
        ExpressionKind::Root => None,
    }
}

/// A symbol designator as the reader would fold it.
fn normalize(name: &str) -> String {
    name.trim_start_matches("#:")
        .trim_start_matches(':')
        .trim_matches('|')
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<PackageLock> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_package_lock_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn redefining_a_standard_function_is_reported() {
        let report = report("(defun list (&rest xs) xs)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].collision, Collision::Redefinition);
        assert_eq!(report.findings[0].name, "LIST");
    }

    #[test]
    fn defining_an_ordinary_name_is_not_reported() {
        let report = report("(defun render-pane (x) x)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn binding_a_standard_name_is_reported_separately_from_redefining_one() {
        let report = report("(defun f () (let ((list 1)) list))");
        assert_eq!(report.findings[0].collision, Collision::Binding);
    }

    #[test]
    fn an_explicit_shadow_is_reported_but_not_as_undefined_behavior() {
        let report = report("(defpackage :app (:use :cl) (:shadow #:list))");
        assert_eq!(report.findings[0].collision, Collision::Shadow);
        assert_eq!(report.summary, vec![("undefined_behavior_count", json!(0))]);
    }

    #[test]
    fn a_designator_is_normalized_before_it_is_looked_up() {
        for source in [
            "(defpackage :app (:shadow #:list))",
            "(defpackage :app (:shadow :list))",
            "(defpackage :app (:shadow |list|))",
        ] {
            let report = report(source);
            assert_eq!(report.findings.len(), 1, "{source}");
            assert_eq!(report.findings[0].name, "LIST", "{source}");
        }
    }

    #[test]
    fn a_standard_symbol_is_matched_case_insensitively() {
        assert_eq!(report("(defun LIST () 1)").findings.len(), 1);
    }

    #[test]
    fn a_macro_redefinition_counts_too() {
        let report = report("(defmacro when (test &body body) `(if ,test (progn ,@body)))");
        assert_eq!(report.findings[0].collision, Collision::Redefinition);
    }

    #[test]
    fn a_file_that_touches_no_standard_symbol_reports_nothing() {
        let report = report("(defun render (x) (let ((pane x)) pane))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defn list [x] x)", Dialect::Clojure).expect("parse");
        let report = build_package_lock_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
