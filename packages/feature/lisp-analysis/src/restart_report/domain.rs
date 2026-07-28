//! Restarts established against restarts invoked, and each side with no
//! counterpart.
//!
//! The condition system is the one part of Common Lisp where the two halves of
//! a contract are routinely written in different files by different people. A
//! `restart-case` establishes named recovery strategies; a handler somewhere up
//! the stack calls `invoke-restart` on one by name. Nothing connects them but
//! the symbol, and nothing checks the symbol.
//!
//! Both failure directions are real and neither is visible locally:
//!
//! - `(invoke-restart 'retyr)` — a typo — signals `control-error` at the moment
//!   the handler runs, which is the moment things were already going wrong.
//! - A `restart-case` clause nobody invokes is dead recovery code. It looks
//!   like resilience and provides none.
//!
//! `handler-bind` clauses are collected too, because a restart is only reachable
//! from a handler: a file that establishes restarts and binds no handler has
//! written the offer without the acceptance.

use std::collections::BTreeSet;
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// What one finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartRole {
    /// A `restart-case`/`restart-bind` clause that something invokes.
    Established,
    /// An `invoke-restart` naming a restart something establishes.
    Invoked,
    /// A restart established and never invoked anywhere analyzed.
    Uninvoked,
    /// An `invoke-restart` naming a restart nothing analyzed establishes.
    Unestablished,
    /// A `handler-bind`/`handler-case` clause, reported so the two halves of
    /// the condition system can be seen together.
    Handler,
}

impl RestartRole {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Established => "established",
            Self::Invoked => "invoked",
            Self::Uninvoked => "uninvoked",
            Self::Unestablished => "unestablished",
            Self::Handler => "handler",
        }
    }

    /// Whether this role names a broken pairing.
    ///
    /// Note the asymmetry: an unestablished invocation is a `control-error`
    /// waiting to happen, while an uninvoked restart may legitimately be
    /// invoked interactively by a user at the debugger. Both are reported;
    /// only the first is unambiguous, which is why the gate can be narrowed.
    #[must_use]
    pub const fn is_unpaired(self) -> bool {
        matches!(self, Self::Uninvoked | Self::Unestablished)
    }
}

/// One restart, invocation, or handler clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartFinding {
    pub role: RestartRole,
    /// The restart name, or the condition type for a handler clause.
    pub name: String,
    /// The form that established or invoked it.
    pub form: String,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for RestartFinding {
    fn kind(&self) -> &'static str {
        self.role.label()
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
            ("role", json!(self.role.label())),
            ("name", json!(self.name)),
            ("form", json!(self.form)),
        ]
    }
}

/// A finding before the two sides are matched against each other.
struct Raw {
    role: RestartRole,
    name: String,
    form: String,
    span: ByteSpan,
}

#[must_use]
pub fn build_restart_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<RestartFinding> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut raw = Vec::new();

    if modelled {
        collect(&tree.root_view(), &mut raw);
    }

    // Both sides are gathered before either is judged: a restart established
    // after the handler that invokes it is still established, and CLHS imposes
    // no textual order between them.
    let established: BTreeSet<&String> = raw
        .iter()
        .filter(|item| item.role == RestartRole::Established)
        .map(|item| &item.name)
        .collect();
    let invoked: BTreeSet<&String> = raw
        .iter()
        .filter(|item| item.role == RestartRole::Invoked)
        .map(|item| &item.name)
        .collect();

    let findings = raw
        .iter()
        .map(|item| RestartFinding {
            role: match item.role {
                RestartRole::Established if !invoked.contains(&item.name) => RestartRole::Uninvoked,
                RestartRole::Invoked if !established.contains(&item.name) => {
                    RestartRole::Unestablished
                }
                role => role,
            },
            name: item.name.clone(),
            form: item.form.clone(),
            span: item.span,
            line: line_of(source, item.span.start().get()),
        })
        .collect::<Vec<_>>();

    let unpaired = findings
        .iter()
        .filter(|finding| finding.role.is_unpaired())
        .count();
    let handlers = findings
        .iter()
        .filter(|finding| finding.role == RestartRole::Handler)
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![
            ("handler_count", json!(handlers)),
            ("unpaired_count", json!(unpaired)),
        ],
    )
}

fn collect(view: &ExpressionView, raw: &mut Vec<Raw>) {
    if let Some(head) = list_head(view) {
        if ["restart-case", "restart-bind"]
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            // The protected form is child 1; every clause after it names a
            // restart in head position.
            for clause in view.children.iter().skip(2) {
                if let Some(name) = clause.children.first().and_then(atom_symbol_text) {
                    raw.push(Raw {
                        role: RestartRole::Established,
                        name: fold(name),
                        form: head.to_owned(),
                        span: clause.span,
                    });
                }
            }
        } else if ["handler-case", "handler-bind"]
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            for clause in handler_clauses(view, head) {
                if let Some(name) = clause.children.first().and_then(atom_symbol_text) {
                    raw.push(Raw {
                        role: RestartRole::Handler,
                        name: fold(name),
                        form: head.to_owned(),
                        span: clause.span,
                    });
                }
            }
        } else if common_lisp_operator_head_eq(head, "invoke-restart") {
            if let Some(name) = view.children.get(1).and_then(restart_designator) {
                raw.push(Raw {
                    role: RestartRole::Invoked,
                    name,
                    form: head.to_owned(),
                    span: view.span,
                });
            }
        } else if let Some(name) = shorthand_invocation(head) {
            // `(abort)`, `(continue)`, `(muffle-warning)`, `(store-value v)`
            // and `(use-value v)` invoke the standard restart of the same
            // name. Not recognising them would report every `(continue)`
            // restart clause as uninvoked.
            raw.push(Raw {
                role: RestartRole::Invoked,
                name,
                form: head.to_owned(),
                span: view.span,
            });
        }
    }

    for child in &view.children {
        collect(child, raw);
    }
}

/// A `handler-bind`'s clauses live inside its binding list; a `handler-case`'s
/// are its own trailing children.
fn handler_clauses<'a>(view: &'a ExpressionView, head: &str) -> &'a [ExpressionView] {
    if common_lisp_operator_head_eq(head, "handler-bind") {
        view.children
            .get(1)
            .map_or(&[][..], |bindings| bindings.children.as_slice())
    } else {
        view.children.get(2..).unwrap_or_default()
    }
}

/// The restart named by an `invoke-restart` argument.
///
/// Only a literal name is read. A computed designator is a restart this
/// analysis cannot name, and inventing one would produce a finding about a
/// symbol that appears nowhere in the file.
fn restart_designator(view: &ExpressionView) -> Option<String> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = atom_symbol_text(view)?;
    Some(fold(text.trim_start_matches('\'')))
}

/// The standard restart a bare call invokes.
fn shorthand_invocation(head: &str) -> Option<String> {
    [
        "abort",
        "continue",
        "muffle-warning",
        "store-value",
        "use-value",
    ]
    .iter()
    .find(|name| common_lisp_operator_head_eq(head, name))
    .map(|name| fold(name))
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<RestartFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_restart_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn roles(report: &FileFindings<RestartFinding>) -> Vec<RestartRole> {
        report.findings.iter().map(|finding| finding.role).collect()
    }

    #[test]
    fn a_restart_nothing_invokes_is_reported() {
        let report = report("(defun f () (restart-case (g) (retry () (f))))");
        assert_eq!(roles(&report), vec![RestartRole::Uninvoked]);
        assert_eq!(report.findings[0].name, "RETRY");
    }

    #[test]
    fn an_invocation_with_no_establishment_is_reported() {
        let report = report("(defun f () (invoke-restart 'retyr))");
        assert_eq!(roles(&report), vec![RestartRole::Unestablished]);
    }

    #[test]
    fn a_paired_restart_and_invocation_are_both_sound() {
        let report = report(
            "(defun f () (restart-case (g) (retry () 1)))\n\
             (defun h () (invoke-restart 'retry))",
        );
        assert_eq!(
            roles(&report),
            vec![RestartRole::Established, RestartRole::Invoked]
        );
        assert_eq!(report.summary[1], ("unpaired_count", json!(0)));
    }

    #[test]
    fn a_restart_established_after_its_invocation_still_pairs() {
        let report = report(
            "(defun h () (invoke-restart 'retry))\n\
             (defun f () (restart-case (g) (retry () 1)))",
        );
        assert_eq!(report.summary[1], ("unpaired_count", json!(0)));
    }

    #[test]
    fn a_bare_continue_invokes_the_standard_restart_of_that_name() {
        let report =
            report("(defun f () (restart-case (g) (continue () 1)))\n(defun h () (continue))");
        assert_eq!(report.summary[1], ("unpaired_count", json!(0)));
    }

    #[test]
    fn a_handler_bind_clause_is_reported_beside_the_restarts() {
        let report = report("(defun f () (handler-bind ((error #'log)) (g)))");
        assert_eq!(roles(&report), vec![RestartRole::Handler]);
        assert_eq!(report.findings[0].name, "ERROR");
    }

    #[test]
    fn a_handler_case_clause_is_reported_too() {
        let report = report("(defun f () (handler-case (g) (error (e) e)))");
        assert_eq!(roles(&report), vec![RestartRole::Handler]);
    }

    #[test]
    fn a_computed_restart_designator_is_not_invented_as_a_name() {
        let report = report("(defun f (r) (invoke-restart (find-restart r)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_file_with_no_condition_handling_reports_nothing() {
        let report = report("(defun f () 1)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(try (f) (catch Exception e e))", Dialect::Clojure)
                .expect("parse");
        let report = build_restart_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
