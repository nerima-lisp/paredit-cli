//! Flow narrowing: what a predicate or type-case test proves within one
//! branch, and what a branching form's own type is once its branches are
//! known.
//!
//! Only the narrowing form's own head is classified here; every subform is
//! still routed back through [`super::inference::infer_view`], so a nested
//! `the`, `declare`, or narrowing form inside a branch is handled exactly as
//! it would be anywhere else.

use std::collections::HashMap;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

use crate::semantics::NodeKey;
use crate::semantics::binding::BindingId;

use super::super::model::{Ty, Type, TypeTableBuilder, join, meet};
use super::super::policy::narrowed_by_predicate;
use super::inference::{Ctx, infer_view};

/// Bindings narrowed along the current path, meet-combined as forms nest.
///
/// Cloned rather than mutated in place: an `if`'s `then` and `else` branch —
/// or a `cond`/`typecase` clause and its next sibling — each need their own
/// narrowing built on the *same* starting point, not one another's.
#[derive(Clone, Default)]
pub(super) struct Narrowing(HashMap<BindingId, Ty>);

impl Narrowing {
    pub(super) fn get(&self, binding: BindingId) -> Option<Ty> {
        self.0.get(&binding).copied()
    }

    /// This narrowing plus `binding: ty`, meeting with whatever this path
    /// already knew about `binding` — nested tests narrow further, they never
    /// widen back out.
    fn refine(&self, binding: BindingId, ty: Ty) -> Self {
        let mut next = self.0.clone();
        let merged = next
            .get(&binding)
            .map_or(ty, |existing| meet(*existing, ty));
        next.insert(binding, merged);
        Self(next)
    }
}

/// The narrowing/merging forms this module knows how to handle. Every other
/// head — including the other standard control forms, `progn`, `case`, … —
/// falls through to [`super::calls`], which still descends into them but
/// makes no narrowing or merge claim.
#[derive(Clone, Copy)]
pub(super) enum ControlForm {
    If,
    When,
    Unless,
    Cond,
    Typecase,
}

pub(super) fn classify(head: &str) -> Option<ControlForm> {
    for (name, form) in [
        ("if", ControlForm::If),
        ("when", ControlForm::When),
        ("unless", ControlForm::Unless),
        ("cond", ControlForm::Cond),
        ("typecase", ControlForm::Typecase),
        ("etypecase", ControlForm::Typecase),
    ] {
        if common_lisp_operator_head_eq(head, name) {
            return Some(form);
        }
    }
    None
}

pub(super) fn infer(
    ctx: &Ctx,
    narrowing: &Narrowing,
    form: ControlForm,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    match form {
        ControlForm::If => infer_if(ctx, narrowing, view, builder),
        ControlForm::When => {
            infer_when_unless(ctx, narrowing, view, builder, true);
            Type::Unknown
        }
        ControlForm::Unless => {
            infer_when_unless(ctx, narrowing, view, builder, false);
            Type::Unknown
        }
        ControlForm::Cond => infer_cond(ctx, narrowing, view, builder),
        ControlForm::Typecase => infer_typecase(ctx, narrowing, view, builder),
    }
}

/// What `test` proves about its argument, if `test` is `(predicate x)` for a
/// bare, resolvable `x` and a predicate this layer knows.
fn predicate_narrowing(ctx: &Ctx, test: &ExpressionView, holds: bool) -> Option<(BindingId, Ty)> {
    if test.kind != ExpressionKind::List || test.children.len() != 2 {
        return None;
    }
    let predicate = atom_symbol_text(&test.children[0])?;
    let span = atom_symbol_span(&test.children[1])?;
    let binding = ctx.bindings.resolve(NodeKey::atom(span))?;
    let ty = narrowed_by_predicate(predicate, holds)?;
    Some((binding, ty))
}

fn narrowing_for(narrowing: &Narrowing, proof: Option<(BindingId, Ty)>) -> Narrowing {
    proof.map_or_else(
        || narrowing.clone(),
        |(binding, ty)| narrowing.refine(binding, ty),
    )
}

/// `(if test then [else])`. An `if` with no `else` returns `nil` when the
/// test fails, exactly as `value::service::folding` treats it — so a missing
/// `else` joins against [`Ty::Null`] rather than dropping out of the merge.
fn infer_if(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    let args = &view.children[1..];
    let Some(test) = args.first() else {
        return Type::Unknown;
    };
    infer_view(ctx, narrowing, test, builder);

    let then_narrowing = narrowing_for(narrowing, predicate_narrowing(ctx, test, true));
    let else_narrowing = narrowing_for(narrowing, predicate_narrowing(ctx, test, false));

    let then_ty = args
        .get(1)
        .and_then(|form| infer_view(ctx, &then_narrowing, form, builder).as_known());
    let else_ty = match args.get(2) {
        Some(form) => infer_view(ctx, &else_narrowing, form, builder).as_known(),
        None => Some(Ty::Null),
    };

    match (then_ty, else_ty) {
        (Some(then_ty), Some(else_ty)) => Type::Known(join(then_ty, else_ty)),
        _ => Type::Unknown,
    }
}

/// `(when test body…)` / `(unless test body…)`: narrows `body` but makes no
/// merge claim about the form's own value — unlike `if`, there is no second
/// branch to join against, and CLHS gives the failing case an unmodelled
/// value only when [`Ty::Null`] would not obviously be it (`when`'s failing
/// case *is* `nil`, but nothing here consumes that fact, so it is left out
/// rather than guessed at).
fn infer_when_unless(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
    holds_when_taken: bool,
) {
    let args = &view.children[1..];
    let Some(test) = args.first() else {
        return;
    };
    infer_view(ctx, narrowing, test, builder);

    let body_narrowing = narrowing_for(narrowing, predicate_narrowing(ctx, test, holds_when_taken));
    for form in &args[1..] {
        infer_view(ctx, &body_narrowing, form, builder);
    }
}

/// `(cond (test body…)…)`: each clause narrows its own body by its test, and
/// the form's type is the join over every clause — provided every clause's
/// result is known, since an unmatched fall-through (no catch-all clause) is
/// not modelled and must not be guessed at as `nil`.
fn infer_cond(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    let mut joined: Option<Ty> = None;
    let mut unknown = false;

    for clause in &view.children[1..] {
        if clause.kind != ExpressionKind::List || clause.children.is_empty() {
            unknown = true;
            continue;
        }
        let test = &clause.children[0];
        let test_ty = infer_view(ctx, narrowing, test, builder);
        let clause_narrowing = narrowing_for(narrowing, predicate_narrowing(ctx, test, true));

        let body = &clause.children[1..];
        let clause_ty = if body.is_empty() {
            // `(test)` alone evaluates to the test's own value.
            test_ty.as_known()
        } else {
            body.iter()
                .map(|form| infer_view(ctx, &clause_narrowing, form, builder))
                .last()
                .and_then(|ty| ty.as_known())
        };

        match clause_ty {
            Some(ty) => joined = Some(joined.map_or(ty, |acc| join(acc, ty))),
            None => unknown = true,
        }
    }

    if unknown {
        Type::Unknown
    } else {
        joined.map_or(Type::Unknown, Type::Known)
    }
}

/// `(typecase subject (type body…)…)` / `etypecase`: each clause narrows
/// `subject` by its type name — `t`/`otherwise` narrow to nothing, since
/// [`Ty::from_name`] does not recognise `otherwise` and `t` narrows to
/// [`Ty::Top`], the identity for [`meet`] — and the form's type is the join
/// over clauses, under the same all-clauses-known requirement as `cond`.
fn infer_typecase(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    let Some(subject) = view.children.get(1) else {
        return Type::Unknown;
    };
    infer_view(ctx, narrowing, subject, builder);
    let subject_binding =
        atom_symbol_span(subject).and_then(|span| ctx.bindings.resolve(NodeKey::atom(span)));

    let mut joined: Option<Ty> = None;
    let mut unknown = false;

    for clause in &view.children[2..] {
        if clause.kind != ExpressionKind::List || clause.children.is_empty() {
            unknown = true;
            continue;
        }
        let clause_ty_name = atom_symbol_text(&clause.children[0]).and_then(Ty::from_name);
        let clause_narrowing = subject_binding
            .zip(clause_ty_name)
            .map(|(binding, ty)| narrowing.refine(binding, ty))
            .unwrap_or_else(|| narrowing.clone());

        let body = &clause.children[1..];
        let clause_ty = body
            .iter()
            .map(|form| infer_view(ctx, &clause_narrowing, form, builder))
            .last()
            .and_then(|ty| ty.as_known());

        match clause_ty {
            Some(ty) => joined = Some(joined.map_or(ty, |acc| join(acc, ty))),
            None => unknown = true,
        }
    }

    if unknown {
        Type::Unknown
    } else {
        joined.map_or(Type::Unknown, Type::Known)
    }
}
