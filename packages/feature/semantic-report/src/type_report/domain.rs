//! What the type layer proved, and where a declaration contradicts it.
//!
//! The type table is the top of the semantic stack and, until this report, was
//! read only by lint rules asking yes/no questions ("is this argument
//! definitely a number?"). Printing it turns the layer into something an agent
//! can plan against.
//!
//! The two halves of the table come from different places, and keeping them
//! apart is what makes the report worth more than a dump:
//!
//! - A **binding**'s type comes only from a declaration — `declare`, `declaim`,
//!   `proclaim`, `check-type`. Inference never writes one.
//! - An **expression**'s type comes only from inference — a literal's value, a
//!   standard function's return type, a `the`, or flow narrowing.
//!
//! What a binding is *actually* worth therefore lives one layer down, in the
//! value table: a `let` initial form is not visited as an expression, so its
//! literal never reaches the type table, but propagation does record the
//! constant on the binding. So the two claims about `x` in
//! `(let ((x 1)) (declare (type string x)) …)` are `string` from the
//! declaration and `integer` from the propagated value, and nothing in the
//! layer ever puts them beside each other. This report does, and a pair whose
//! meet is [`Ty::Bottom`] is a declaration no object can satisfy — a real
//! defect, and one no existing report surfaces.

use std::path::PathBuf;

use paredit_core_semantics::semantics::binding::Binding;
use paredit_core_semantics::semantics::typing::model::meet;
use paredit_core_semantics::semantics::typing::{Ty, Type};
use paredit_core_semantics::semantics::value::PropagatableValue;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteSpan;

use crate::shared::{SemanticFile, line_of, snippet};

/// How much of a form this report quotes back. A `defun` body is not a label,
/// so a finding's snippet is capped rather than unbounded.
const SNIPPET_LIMIT: usize = 48;

/// The label for a type slot that may hold no claim at all.
///
/// [`Ty`] has `as_str`, but `Type::Unknown` is not a `Ty` — the distinction
/// between "no claim" and "the empty type" is exactly what a consumer must not
/// lose, so it is spelled here rather than collapsed onto `nil`.
#[must_use]
pub fn type_label(ty: Type) -> &'static str {
    ty.as_known().map_or("unknown", Ty::as_str)
}

/// One binding, with the declaration's claim and inference's claim side by
/// side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedBinding {
    pub name: String,
    /// What a `declare`/`declaim`/`proclaim`/`check-type` said. `Unknown` when
    /// nothing declared it.
    pub declared: Type,
    /// What the value layer proved this binding is worth. `Unknown` when the
    /// binding is not provably constant — reassigned, special, in an opaque
    /// scope, or initialized from a form the layer cannot evaluate.
    pub inferred: Type,
    pub span: ByteSpan,
    pub line: usize,
    /// The form that bound it (`let`, `defun`, …), when it had a head.
    pub binder: Option<String>,
    /// Whether the two claims provably share no member, so no object can
    /// satisfy both.
    pub contradictory: bool,
}

/// One expression inference has a claim about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedExpression {
    pub ty: Ty,
    pub span: ByteSpan,
    pub line: usize,
    pub text: String,
    /// Whether inference itself reached the empty type, which two disagreeing
    /// sources on one expression (a `the` against a literal) produce.
    pub contradictory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Whether the semantic layer models this dialect at all. `false` with an
    /// empty finding list means "not measured", which is not the same as
    /// "nothing found", and a consumer cannot tell them apart otherwise.
    pub dialect_modelled: bool,
    pub bindings: Vec<TypedBinding>,
    pub expressions: Vec<TypedExpression>,
    /// Bindings for which neither half of the table has a claim. A count
    /// rather than a list: the ratio is the signal, and naming every
    /// unresolved binding would bury the resolved ones.
    pub untyped_binding_count: usize,
}

impl TypeReportFile {
    #[must_use]
    pub fn contradictions(&self) -> usize {
        self.bindings
            .iter()
            .filter(|binding| binding.contradictory)
            .count()
            + self
                .expressions
                .iter()
                .filter(|expression| expression.contradictory)
                .count()
    }
}

#[must_use]
pub fn build_type_report(file: &SemanticFile) -> TypeReportFile {
    let source = file.tree.source();

    let mut bindings = Vec::new();
    let mut untyped_binding_count = 0;
    for (id, binding) in file.bindings.bindings() {
        let declared = file.types.binding_type(id);
        let inferred = file
            .values
            .binding_value(id)
            .map_or(Type::Unknown, |value| Type::Known(value_type(value)));

        if declared == Type::Unknown && inferred == Type::Unknown {
            untyped_binding_count += 1;
            continue;
        }
        bindings.push(typed_binding(binding, declared, inferred, source));
    }
    // The semantic tables are `HashMap`s, and `bindings()` is arena order.
    // Source order is imposed here so two runs over the same bytes print the
    // same bytes.
    bindings.sort_by_key(|binding| (binding.span.start().get(), binding.span.end().get()));

    let mut expressions = file
        .types
        .expression_types()
        .map(|(key, ty)| TypedExpression {
            ty,
            span: key.span(),
            line: line_of(source, key.span().start().get()),
            text: snippet(source, key.span(), SNIPPET_LIMIT),
            contradictory: ty == Ty::Bottom,
        })
        .collect::<Vec<_>>();
    expressions
        .sort_by_key(|expression| (expression.span.start().get(), expression.span.end().get()));

    TypeReportFile {
        path: file.path.clone(),
        dialect: file.dialect,
        dialect_modelled: file.dialect_is_modelled(),
        bindings,
        expressions,
        untyped_binding_count,
    }
}

fn typed_binding(binding: &Binding, declared: Type, inferred: Type, source: &str) -> TypedBinding {
    TypedBinding {
        name: binding.name().as_str().to_owned(),
        declared,
        inferred,
        span: binding.definition(),
        line: line_of(source, binding.definition().start().get()),
        binder: binding.binder_head().map(ToOwned::to_owned),
        contradictory: contradicts(declared, inferred),
    }
}

/// Whether two claims about one binding provably share no member.
///
/// Both must be known: `Unknown` makes no claim, so it cannot contradict one.
/// `Bottom` on either side is already a contradiction the layer recorded.
fn contradicts(declared: Type, inferred: Type) -> bool {
    match (declared.as_known(), inferred.as_known()) {
        (Some(declared), Some(inferred)) => meet(declared, inferred) == Ty::Bottom,
        (Some(Ty::Bottom), _) | (_, Some(Ty::Bottom)) => true,
        _ => false,
    }
}

/// The type of a value the propagation layer proved.
///
/// Total by construction: [`PropagatableValue`] excludes strings and floats,
/// the two literals the layer refuses to carry through a binding, so there is
/// no "unknown" arm to get wrong.
const fn value_type(value: &PropagatableValue) -> Ty {
    match value {
        PropagatableValue::Integer(_) => Ty::Integer,
        PropagatableValue::Char(_) => Ty::Character,
        PropagatableValue::Keyword(_) => Ty::Keyword,
        PropagatableValue::Boolean(_) => Ty::Boolean,
        PropagatableValue::Nil => Ty::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn report_of(source: &str, dialect: Dialect) -> TypeReportFile {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_type_report(&SemanticFile::analyze(Path::new("t.lisp"), dialect, tree))
    }

    fn report(source: &str) -> TypeReportFile {
        report_of(source, Dialect::CommonLisp)
    }

    fn binding<'a>(report: &'a TypeReportFile, name: &str) -> &'a TypedBinding {
        report
            .bindings
            .iter()
            .find(|binding| binding.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is in the report: {report:?}"))
    }

    #[test]
    fn a_propagated_constant_gives_the_binding_an_inferred_type() {
        let report = report("(defun f () (let ((x 1)) x))");
        let x = binding(&report, "x");
        assert_eq!(x.inferred, Type::Known(Ty::Integer));
        assert_eq!(x.declared, Type::Unknown);
        assert!(!x.contradictory);
    }

    #[test]
    fn a_declaration_is_recorded_on_the_binding() {
        let report = report("(defun f (x) (declare (type string x)) x)");
        assert_eq!(binding(&report, "x").declared, Type::Known(Ty::String));
    }

    #[test]
    fn a_declaration_contradicting_the_initial_form_is_reported() {
        let report = report("(defun f () (let ((x 1)) (declare (type string x)) x))");
        let x = binding(&report, "x");
        assert_eq!(x.declared, Type::Known(Ty::String));
        assert_eq!(x.inferred, Type::Known(Ty::Integer));
        assert!(x.contradictory);
        // Two findings, not one: the binding carries the declaration against
        // the propagated value, and the reference site carries what inference
        // reached once the declaration narrowed it — `nil`, the empty type.
        assert_eq!(report.contradictions(), 2);
        assert!(
            report
                .expressions
                .iter()
                .any(|expression| expression.contradictory)
        );
    }

    #[test]
    fn a_declaration_agreeing_with_the_initial_form_is_not_a_contradiction() {
        let report = report("(defun f () (let ((x 1)) (declare (type integer x)) x))");
        let x = binding(&report, "x");
        assert_eq!(x.declared, Type::Known(Ty::Integer));
        assert!(!x.contradictory);
    }

    #[test]
    fn a_declaration_widening_the_initial_form_is_not_a_contradiction() {
        // `integer` is a subtype of `number`, so both claims hold at once.
        let report = report("(defun f () (let ((x 1)) (declare (type number x)) x))");
        assert!(!binding(&report, "x").contradictory);
    }

    #[test]
    fn a_binding_records_the_form_that_bound_it() {
        let report = report("(defun f () (let ((x 1)) x))");
        assert_eq!(binding(&report, "x").binder.as_deref(), Some("let"));
    }

    #[test]
    fn every_propagatable_literal_maps_to_a_type() {
        assert_eq!(value_type(&PropagatableValue::Integer(1)), Ty::Integer);
        assert_eq!(value_type(&PropagatableValue::Char('a')), Ty::Character);
        assert_eq!(value_type(&PropagatableValue::Boolean(true)), Ty::Boolean);
        assert_eq!(value_type(&PropagatableValue::Nil), Ty::Null);
    }

    #[test]
    fn a_binding_with_no_claim_on_either_side_is_counted_not_listed() {
        let report = report("(defun f (a) (let ((x (unknown-call a))) x))");
        assert!(
            report
                .bindings
                .iter()
                .all(|binding| !binding.name.eq_ignore_ascii_case("x")),
            "{report:?}"
        );
        assert!(report.untyped_binding_count >= 1, "{report:?}");
    }

    #[test]
    fn findings_are_sorted_by_source_position() {
        let report = report("(defun f () (let ((a 1) (b \"s\") (c #\\x)) (list a b c)))");
        let starts = report
            .bindings
            .iter()
            .map(|binding| binding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn expression_findings_are_sorted_by_source_position() {
        let report = report("(defun f () (+ 1 (* 2 3)))");
        let starts = report
            .expressions
            .iter()
            .map(|expression| expression.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let report = report_of("(let [x 1] x)", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.bindings.is_empty());
        assert!(report.expressions.is_empty());
    }

    #[test]
    fn unknown_makes_no_claim_so_it_cannot_contradict_one() {
        assert!(!contradicts(Type::Unknown, Type::Known(Ty::String)));
        assert!(!contradicts(Type::Known(Ty::String), Type::Unknown));
        assert!(!contradicts(Type::Unknown, Type::Unknown));
    }

    #[test]
    fn the_label_keeps_no_claim_and_the_empty_type_apart() {
        assert_eq!(type_label(Type::Unknown), "unknown");
        assert_eq!(type_label(Type::Known(Ty::Bottom)), "nil");
    }
}
