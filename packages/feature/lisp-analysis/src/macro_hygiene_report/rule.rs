//! `macro-variable-capture`: the fixable half of `inspect macro-hygiene`.
//!
//! [`crate::macro_hygiene_report::domain`] backs both this rule and the
//! standalone `inspect macro-hygiene` report, and reports five risks; this
//! rule surfaces only [`HygieneRisk::VariableCapture`] through `fix
//! apply`/`inspect lint --fix`; per its own FR the round that added this rule
//! left the other four report-only. Multiple evaluation in particular is
//! deliberately not fixed here: an automatic `once-only` consolidation is a
//! larger, separate rewrite than gensym insertion, and is out of scope for
//! this rule.
//!
//! The fix itself is scoped narrower still, to the one shape it can rewrite
//! with confidence: a template whose own root is a `let`/`let*` form with
//! exactly one binding, the same shape [`crate::macro_hygiene_report::domain`]'s
//! own module docs use as the running example. Every other capture shape —
//! more than one binding in the same list, a capture nested deeper than the
//! template's own root — is still reported, just without a fix attached: a
//! rule tagged [`Fixability::Fixable`] is allowed to leave an individual
//! finding without a [`RuleFix`] when it cannot rewrite it with confidence,
//! and a wrong guess here would rewrite working code.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::definition::is_macro_expander_definition;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::macro_hygiene_report::domain::{
    self, HYGIENE_MODELLED_DIALECTS, HygieneFinding, HygieneRisk, hygiene_findings_in,
};

pub const META: RuleMeta = RuleMeta::new(
    "macro-variable-capture",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a defmacro template binds a literal name that is not obviously a gensym",
    Fixability::Fixable,
);

/// Every macro-definition head [`is_macro_expander_definition`] recognises
/// across [`HYGIENE_MODELLED_DIALECTS`]. A pre-filter only: `check` still
/// confirms the exact dialect/head pairing before doing anything, since e.g.
/// `macro` means something in Fennel and nothing in the other seven.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("cl-defmacro"),
    NormalizedHead::new("macro"),
    NormalizedHead::new("defsyntax"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&HYGIENE_MODELLED_DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let dialect = context.dialect();
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        if !is_macro_expander_definition(dialect, head) {
            return Ok(());
        }
        let Some(name) = view.children.get(1).and_then(atom_symbol_text) else {
            return Ok(());
        };

        for finding in hygiene_findings_in(dialect, view, name) {
            if finding.risk != HygieneRisk::VariableCapture {
                continue;
            }
            let message = format!(
                "variable capture: template binds `{}` to a literal name that is not \
                 obviously a gensym",
                finding.subject
            );
            match capture_fix(context, view, &finding) {
                Some(fix) => sink.report_fixed(finding.span, message, fix),
                None => sink.report(finding.span, message),
            }
        }
        Ok(())
    }
}

/// `let`/`let*` only: the value-binding forms a gensym prelude makes sense
/// for. `flet`/`labels` bind function names to lambda lists, which this
/// rewrite does not apply to.
const AUTO_FIX_BINDING_FORMS: [&str; 2] = ["let", "let*"];

/// The one shape this rule can rewrite with confidence: a quasiquoted
/// `let`/`let*` template with exactly one binding, matching `finding`'s span.
struct FixableCapture<'a> {
    /// The whole quasiquoted template — the `` `(let ...) `` form itself.
    template: &'a ExpressionView,
    /// The captured name, as written (bare, not yet unquoted).
    binding_name: &'a ExpressionView,
}

/// Builds the automatic rewrite for one `VariableCapture` finding, or `None`
/// when the finding's shape is not the narrow one this rule rewrites.
///
/// The rewrite: bind the captured name to `(gensym)` in a new `let` wrapped
/// *outside* the template, then unquote every bare reference to that name
/// still inside the template (the binding itself, plus any other bare
/// reference in the `let`'s body) so the expansion still reads and writes
/// the same fresh symbol.
fn capture_fix(
    context: &RuleContext<'_>,
    form: &ExpressionView,
    finding: &HygieneFinding,
) -> Option<RuleFix> {
    let shape = find_fixable_capture(form, finding.span)?;
    let name_text = context.slice(shape.binding_name.span);
    let folded_name = name_text.to_ascii_uppercase();

    let mut edits = vec![Replacement::new(
        shape.binding_name.span,
        format!(",{name_text}"),
    )];

    // Every other bare (not already unquoted) reference to the same name in
    // the `let`'s own body — the binding list itself is skipped, since its
    // one binding is exactly what the first edit above already rewrites.
    for body in shape.template.children.iter().skip(2) {
        domain::walk(body, &mut |node| {
            if node.kind != ExpressionKind::Atom || domain::is_unquoted(node) {
                return;
            }
            let Some(text) = atom_symbol_text(node) else {
                return;
            };
            if text.to_ascii_uppercase() == folded_name {
                edits.push(Replacement::new(node.span, format!(",{text}")));
            }
        });
    }

    let template_start = shape.template.span.start();
    let template_end = shape.template.span.end();
    edits.push(Replacement::new(
        ByteSpan::new(template_start, template_start),
        format!("(let (({name_text} (gensym))) "),
    ));
    edits.push(Replacement::new(
        ByteSpan::new(template_end, template_end),
        ")".to_owned(),
    ));

    edits.sort_by_key(|edit| edit.span().start());
    let mut edits = edits.into_iter();
    let first = edits.next()?;
    Some(RuleFix::multi(
        format!("Bind `{name_text}` via `(gensym)` outside the template"),
        first,
        edits,
    ))
}

fn find_fixable_capture<'a>(
    view: &'a ExpressionView,
    target_span: ByteSpan,
) -> Option<FixableCapture<'a>> {
    if domain::is_quasiquoted(view) {
        if let Some(shape) = fixable_capture_at(view, target_span) {
            return Some(shape);
        }
    }
    for child in &view.children {
        if let Some(found) = find_fixable_capture(child, target_span) {
            return Some(found);
        }
    }
    None
}

/// Whether `view` itself — already known to be quasiquoted — is the narrow
/// `let`/`let*`-with-one-binding shape, and that one binding is at
/// `target_span`.
fn fixable_capture_at<'a>(
    view: &'a ExpressionView,
    target_span: ByteSpan,
) -> Option<FixableCapture<'a>> {
    let head = list_head(view)?;
    if !AUTO_FIX_BINDING_FORMS
        .iter()
        .any(|candidate| common_lisp_operator_head_eq(head, candidate))
    {
        return None;
    }
    let bindings = view.children.get(1)?;
    if bindings.children.len() != 1 {
        return None;
    }
    let binding = &bindings.children[0];
    if binding.span != target_span {
        return None;
    }
    let binding_name = domain::binding_target(binding)?;
    if binding_name.kind != ExpressionKind::Atom || domain::is_unquoted(binding_name) {
        return None;
    }
    Some(FixableCapture {
        template: view,
        binding_name,
    })
}
