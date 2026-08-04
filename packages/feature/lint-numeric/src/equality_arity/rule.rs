//! `equality-arity`: an eq/eql/equal/equalp call without exactly two arguments.
//!
//! The analysis lives in [`crate::equality_arity::domain`], which also backs the
//! standalone `inspect equality-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::equality_arity::domain::examine_call;
use crate::support::is_eql_type_specifier_at;
use crate::support::is_hard_quoted_at;
use crate::support::is_key_or_binding_position_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// Reported at `Warning`, not `Error`, because its measured precision on real
/// code is zero.
///
/// [`Severity::Error`] means "a likely or certain bug". A genuine `(eq x)` is
/// certainly one — but over 5506 Common Lisp files (5556 listed, 50 unparsable)
/// the rule produces 407 findings and **every one of them is false**: 14 of 14
/// in one adjudication, 120 of 120 in another, and the whole population survives
/// only in three projects — SBCL's own compiler (561 of the 624 same-class
/// findings the suite as a whole produces in these positions), `trivia` (62) and
/// `mgl-pax` (1).
///
/// Four merged PRs took the count from 1981 to 407 by closing structurally
/// different classes — hard-quoted data (#119/#120), CLHS type positions (#121),
/// `case` keys and binding lists (#123). What remains is not a fifth class of
/// the same kind. 312 of the 407 sit in an argument position of a macro that
/// treats that position as data (`deftransform`'s lambda list, `defknown`'s type
/// specifier, `define-vop`'s `:arg-types`, `trivia`'s pattern DSL), and the
/// other 95 are compound type specifiers reached through `or`/`and`/`not`/`cons`
/// combinators plus a long tail of one-off DSL heads.
///
/// No general mechanism closes that. Suppressing inside any head the engine has
/// never seen defined would silence 32886 of the suite's 113979 corpus findings
/// (28.9%) across 186 of the 214 rules that fire at all, and an adjudicated
/// sample of 30 of those found 23 genuine, 5 false, 2 ambiguous — roughly 25000
/// real findings destroyed to remove 405 false ones. A configurable list of
/// "heads whose arguments are data" needs `(head, position)` pairs rather than
/// names, because `deftransform`'s *body* is ordinary code, and it would still
/// leave the 95-finding residue reported. Both leave a build-blocking rule that
/// blocks builds on correct programs.
///
/// So the finding stays — a real `(eq x)` is still worth saying — but it stops
/// being a gate. See `packages/feature/lint-numeric/README.md` if this is ever
/// revisited with a macroexpander behind it, which is what would actually
/// settle which argument positions are evaluated.
pub const META: RuleMeta = RuleMeta::new(
    "equality-arity",
    RuleCategory::Arity,
    Severity::Warning,
    "an eq/eql/equal/equalp call without exactly two arguments",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("eq"),
    NormalizedHead::new("eql"),
    NormalizedHead::new("equal"),
    NormalizedHead::new("equalp"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut call_count = 0;
        let mut items = Vec::new();
        examine_call(view, &mut call_count, &mut items);
        for item in items {
            // `examine_call` already declines a node carrying its *own* `'`, but
            // the arity of a form nested inside an enclosing quote is just as
            // meaningless: `'(cons (eql function) null)` is a type specifier
            // SBCL writes throughout its own sources, and `(eql function)` there
            // is a datum, not a one-argument call. Asked only once a finding
            // exists, so ordinary code never reaches `root_view()`. A
            // quasiquoted `` `(eql ,a ,b) `` is a template that becomes a real
            // call, and stays reported.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            // A `case`-family clause **key** and a variable-binding list are
            // positions that name something rather than call it. CLHS 5.3
            // makes each `case` clause `(keys form*)`, so the `eql` in
            // `(case kind (eql x) …)` is a symbol being compared against and
            // `x` is a body form; `(multiple-value-bind (equal certain) …)`
            // binds two variables. Neither has an arity to be wrong about.
            //
            // Unlike the quote guard this cannot be settled locally, so it
            // walks the ancestors — but, like every guard here, only once a
            // finding already exists, so ordinary code never reaches
            // `root_view()`. A genuine `(eql x)` in a clause *body* is child 1
            // or later of the clause and stays reported.
            if is_key_or_binding_position_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            // `(eql object)` with exactly one argument is a CLHS 4.2.3 compound
            // *type specifier*, and in a type position one argument is not a
            // defect but the only legal spelling: `(defmethod g ((x (eql 7))) …)`
            // and `(typecase x ((eql 5) …))` are not one-argument calls.
            //
            // Narrowed to `eql`-at-one-argument on purpose, because that is
            // exactly what CLHS makes a specifier. `eq`, `equal` and `equalp`
            // name no type at all and `(eql a b c)` names none either, so no
            // other misarity shape can be silenced by this. The ancestor walk
            // runs only once a finding exists, so ordinary code never reaches
            // `root_view()`.
            if item.argument_count == 1
                && item.operator.eq_ignore_ascii_case("eql")
                && is_eql_type_specifier_at(context.tree(), span)
            {
                continue;
            }

            sink.report(
                span,
                format!(
                    "{} takes exactly 2 arguments but has {}",
                    item.operator, item.argument_count
                ),
            );
        }
        Ok(())
    }
}
