//! Ordinary calls: standard-function and locally declared-`ftype` results.

use paredit_core_syntax::common_lisp::{CommonLispOperator, common_lisp_operator_head_eq};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::scheme::scheme_head_has_registered_semantics;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::semantics::binding::policy::{is_pure_standard_function, is_standard_control_form};

use super::super::model::{Type, TypeTableBuilder};
use super::super::policy::standard_return_type;
use super::inference::{Ctx, infer_view};
use super::narrowing::Narrowing;

/// Whether `head` names a form whose children are worth descending into at
/// all — mirrors
/// `binding::service::opacity::head_has_registered_semantics`, which cannot
/// be reused directly (it is private to the binding builder), plus `quote`
/// excluded on top.
///
/// [`CommonLispOperator`] covers `let`, `defun`, `flet`, and every other
/// binder: its lambda list and body are genuinely evaluated in place, so a
/// `the`/`declare`/`check-type`/narrowing form written inside one still needs
/// to be found. The two standard-function/control-form tables cover the rest.
/// `quote` is excluded even though `is_standard_control_form` lists it: it is
/// listed there because it cannot hide a reassignment, not because its
/// argument is evaluated — `(quote (+ 1 2))` denotes the list `(+ 1 2)`, not
/// the integer 3.
///
/// `CommonLispOperator`'s table covers Scheme and Emacs Lisp too wherever
/// they share Common Lisp's own spelling (`let`, `lambda`, `defun`, `progn`,
/// …), which is most of the time, but not `define` or `begin` — Scheme's and
/// Racket's own binder and sequencing spellings. Scheme's own registered-head
/// table covers those, so it is consulted too, but only for the two dialects
/// it actually describes: a head that happens to collide with a Scheme
/// special form's spelling in some other dialect is not evidence of anything
/// there.
///
/// An unrecognized head stays undescended — it may be a macro, and a macro is
/// free not to evaluate what is written inside it at all, exactly the
/// concern `value::service::folding` raises for the same case.
fn evaluates_its_children(dialect: Dialect, head: &str) -> bool {
    if common_lisp_operator_head_eq(head, "quote") {
        return false;
    }
    CommonLispOperator::from_head(head).is_some()
        || is_pure_standard_function(head)
        || is_standard_control_form(head)
        || (matches!(dialect, Dialect::Scheme | Dialect::Racket)
            && scheme_head_has_registered_semantics(head))
}

/// A call (or any other list form `inference`/`narrowing` did not otherwise
/// dispatch): descends into children only when `head` is known to evaluate
/// them in place, and looks up a return type from the file's own `ftype`
/// declarations or the standard-function table.
pub(super) fn infer_call(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    head: &str,
    builder: &mut TypeTableBuilder,
) -> Type {
    if evaluates_its_children(ctx.dialect, head) {
        for argument in &view.children[1..] {
            infer_view(ctx, narrowing, argument, builder);
        }
    }

    ctx.declared_returns
        .get(&head.to_ascii_lowercase())
        .copied()
        .or_else(|| standard_return_type(head))
        .map_or(Type::Unknown, Type::Known)
}
