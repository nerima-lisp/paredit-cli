//! How far constant propagation reached, and what stopped it everywhere else.
//!
//! The value table is a set of bindings that provably carry a constant. Every
//! other binding is simply absent, which makes the table useless for the
//! question an agent actually has: *why* can this binding not be propagated?
//! "Not constant" and "constant, but reassigned on line 40" call for different
//! work, and the table cannot tell them apart.
//!
//! [`Binding::is_propagatable`] checks four conditions in order, and this
//! module reports the first one a binding fails — the same order, so a
//! finding names exactly the fact that disqualified it rather than an
//! arbitrary one of several.

use std::path::PathBuf;

use paredit_core_cli::report::line_of;
use paredit_core_semantics::semantics::binding::{Binding, BindingKind};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteSpan;

use crate::constant_report::domain::literal_text;
use crate::shared::SemanticFile;
use paredit_core_semantics::semantics::value::LiteralValue;

/// Why a binding's value did not reach the value table.
///
/// The variants mirror [`Binding::is_propagatable`]'s checks in the same
/// order, so a binding lands in the bucket that explains the *first*
/// disqualifying fact about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockedReason {
    /// Not a variable at all — a local function, macro, symbol macro, or slot.
    /// There is no value to propagate.
    NotAVariable,
    /// `setq`/`incf`/`push`/… reassigns it somewhere in its scope, so the
    /// initial form is not what a later reference sees.
    Reassigned,
    /// Declared `special`, so it has dynamic extent and any call in the body
    /// can rebind it without a textual reference.
    Special,
    /// Its scope encloses an unknown macro call, quoted data, or a reader
    /// conditional, any of which can do something no traversal sees.
    OpaqueScope,
    /// Nothing to propagate: the binder gave it no initial form.
    NoInitialForm,
    /// Propagatable in principle, but the initial form is not one the value
    /// layer can evaluate — a call, a variable, a string, a float.
    InitialFormNotConstant,
}

impl BlockedReason {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotAVariable => "not-a-variable",
            Self::Reassigned => "reassigned",
            Self::Special => "special",
            Self::OpaqueScope => "opaque-scope",
            Self::NoInitialForm => "no-initial-form",
            Self::InitialFormNotConstant => "initial-form-not-constant",
        }
    }

    /// Every reason, so a summary can report a zero as a zero rather than by
    /// omitting the key.
    pub const ALL: [Self; 6] = [
        Self::NotAVariable,
        Self::Reassigned,
        Self::Special,
        Self::OpaqueScope,
        Self::NoInitialForm,
        Self::InitialFormNotConstant,
    ];
}

/// A binding the value layer resolved to a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropagatedBinding {
    pub name: String,
    pub value: String,
    pub span: ByteSpan,
    pub line: usize,
    pub binder: Option<String>,
    /// How many references would see the constant. Zero means the binding is
    /// constant and unused, which is a `remove-unused-binding` candidate.
    pub reference_count: usize,
}

/// A binding the value layer could not resolve, and the first reason why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedBinding {
    pub name: String,
    pub reason: BlockedReason,
    pub span: ByteSpan,
    pub line: usize,
    pub binder: Option<String>,
    /// Where the scope lost transparency, for [`BlockedReason::OpaqueScope`].
    /// The single actionable detail among the reasons: it names the form to
    /// register or rewrite.
    pub opacity_cause: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePropagationReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub dialect_modelled: bool,
    pub propagated: Vec<PropagatedBinding>,
    pub blocked: Vec<BlockedBinding>,
}

impl ValuePropagationReportFile {
    /// Bindings resolved over bindings seen, in the range 0.0–1.0.
    ///
    /// A file with no bindings has coverage 1.0 rather than 0.0: nothing was
    /// missed, and reporting a perfect miss rate for an empty file would drag
    /// any aggregate down for no reason.
    #[must_use]
    pub fn coverage(&self) -> f64 {
        let total = self.propagated.len() + self.blocked.len();
        if total == 0 {
            return 1.0;
        }
        self.propagated.len() as f64 / total as f64
    }

    #[must_use]
    pub fn blocked_by(&self, reason: BlockedReason) -> usize {
        self.blocked
            .iter()
            .filter(|binding| binding.reason == reason)
            .count()
    }
}

#[must_use]
pub fn build_value_propagation_report(file: &SemanticFile) -> ValuePropagationReportFile {
    let source = file.tree.source();
    let mut propagated = Vec::new();
    let mut blocked = Vec::new();

    for (id, binding) in file.bindings.bindings() {
        let line = line_of(source, binding.definition().start().get());
        match file.values.binding_value(id) {
            Some(value) => propagated.push(PropagatedBinding {
                name: binding.name().as_str().to_owned(),
                value: literal_text(&LiteralValue::from(value.clone())),
                span: binding.definition(),
                line,
                binder: binding.binder_head().map(ToOwned::to_owned),
                reference_count: binding.references().len(),
            }),
            None => blocked.push(BlockedBinding {
                name: binding.name().as_str().to_owned(),
                reason: blocked_reason(binding),
                span: binding.definition(),
                line,
                binder: binding.binder_head().map(ToOwned::to_owned),
                opacity_cause: binding.opacity_cause().map(|cause| cause.kind().label()),
            }),
        }
    }

    // `bindings()` is arena order. Source order is imposed here so two runs
    // over the same bytes print the same bytes.
    propagated.sort_by_key(|binding| (binding.span.start().get(), binding.span.end().get()));
    blocked.sort_by_key(|binding| (binding.span.start().get(), binding.span.end().get()));

    ValuePropagationReportFile {
        path: file.path.clone(),
        dialect: file.dialect,
        dialect_modelled: file.dialect_is_modelled(),
        propagated,
        blocked,
    }
}

/// The first condition this binding fails.
///
/// Order matters and is not alphabetical: it mirrors `is_propagatable`, so
/// a reassigned special reports `Reassigned` — the fact a caller would fix
/// first — rather than whichever check happened to be written last.
fn blocked_reason(binding: &Binding) -> BlockedReason {
    if binding.kind() != BindingKind::Variable {
        return BlockedReason::NotAVariable;
    }
    if !binding.assignments().is_empty() {
        return BlockedReason::Reassigned;
    }
    if !binding.special().is_lexical() {
        return BlockedReason::Special;
    }
    if !binding.opacity().is_transparent() {
        return BlockedReason::OpaqueScope;
    }
    if binding.init_form().is_none() {
        return BlockedReason::NoInitialForm;
    }
    BlockedReason::InitialFormNotConstant
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn report_of(source: &str, dialect: Dialect) -> ValuePropagationReportFile {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_value_propagation_report(&SemanticFile::analyze(Path::new("t.lisp"), dialect, tree))
    }

    fn report(source: &str) -> ValuePropagationReportFile {
        report_of(source, Dialect::CommonLisp)
    }

    fn reason(report: &ValuePropagationReportFile, name: &str) -> BlockedReason {
        report
            .blocked
            .iter()
            .find(|binding| binding.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is blocked: {report:?}"))
            .reason
    }

    #[test]
    fn a_literal_binding_propagates() {
        let report = report("(defun f () (let ((x 1)) x))");
        let x = report
            .propagated
            .iter()
            .find(|binding| binding.name.eq_ignore_ascii_case("x"))
            .expect("x propagates");
        assert_eq!(x.value, "1");
        assert_eq!(x.reference_count, 1);
    }

    #[test]
    fn a_reassigned_binding_reports_the_reassignment() {
        let report = report("(defun f () (let ((x 1)) (setq x 2) x))");
        assert_eq!(reason(&report, "x"), BlockedReason::Reassigned);
    }

    #[test]
    fn a_parameter_has_no_initial_form() {
        let report = report("(defun f (a) a)");
        assert_eq!(reason(&report, "a"), BlockedReason::NoInitialForm);
    }

    #[test]
    fn a_non_constant_initial_form_is_distinguished_from_having_none() {
        let report = report("(defun f (a) (let ((x (identity a))) x))");
        assert_eq!(reason(&report, "x"), BlockedReason::InitialFormNotConstant);
    }

    #[test]
    fn a_local_function_is_not_a_variable() {
        let report = report("(defun f () (flet ((g () 1)) (g)))");
        assert_eq!(reason(&report, "g"), BlockedReason::NotAVariable);
    }

    #[test]
    fn an_unknown_macro_in_scope_blocks_on_opacity() {
        let report = report("(defun f () (let ((x 1)) (mystery-macro) x))");
        assert_eq!(reason(&report, "x"), BlockedReason::OpaqueScope);
    }

    #[test]
    fn coverage_is_resolved_over_seen() {
        let report = report("(defun f (a) (let ((x 1)) (list a x)))");
        // One propagated (`x`), two blocked (`f`, `a`).
        assert!(
            report.coverage() > 0.0 && report.coverage() < 1.0,
            "{report:?}"
        );
    }

    #[test]
    fn a_file_with_no_bindings_is_not_reported_as_a_total_miss() {
        let report = report("(in-package :cl-user)");
        assert_eq!(report.coverage(), 1.0);
    }

    #[test]
    fn findings_are_sorted_by_source_position() {
        let report = report("(defun f () (let ((a 1) (b 2) (c 3)) (list a b c)))");
        let starts = report
            .propagated
            .iter()
            .map(|binding| binding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let report = report_of("(let [x 1] x)", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.propagated.is_empty());
    }

    #[test]
    fn every_reason_has_a_distinct_label() {
        let mut labels = BlockedReason::ALL.map(BlockedReason::label).to_vec();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count);
    }
}
