//! `hy-mutable-default-argument`: a Hy parameter whose default is a fresh
//! mutable object, which every call then shares.
//!
//! Hy compiles `defn` to a Python `FunctionDef`, so a default expression is
//! evaluated **once, at definition time**, and the resulting object is stored
//! on the function. This is the single most-cited Python defect, and it is
//! *not* a Common Lisp defect: `(defun f (&optional (acc (list)))` evaluates
//! its default on entry to each call, which is why the same rule was correctly
//! refuted for Common Lisp in an earlier batch. Hy is the contrast case.
//!
//! Measured against Hy 1.3.1 (CPython 3.14.6):
//!
//! ```text
//! (defn f [[acc []]] (.append acc 1) acc)
//! (print (f) (f) (f))   =>  [1, 1, 1] [1, 1, 1] [1, 1, 1]
//! ```
//!
//! All three calls return the same list. `{}`, `#{}` and `(dict)` behave
//! identically; `None` — the idiom this rule recommends — does not.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::mutable_default_argument::domain::{
    DIALECTS, default_pairs, is_mutable_default, parameter_is_mutated, parameter_list,
};
use crate::shared::{atom_text, is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "hy-mutable-default-argument",
    RuleCategory::Suspicious,
    Severity::Error,
    "a Hy parameter default that every call shares, because Python evaluates it once",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Hy compiles `defn` to a Python `FunctionDef`, so a parameter default is evaluated once \
         when the function is defined and the resulting object lives on the function itself. A \
         mutable default is therefore shared by every call: measured against Hy 1.3.1, \
         `(defn f [[acc []]] (.append acc 1) acc)` returns `[1, 1, 1]` from its third call. Note \
         that this is a *Python* rule, not a Lisp one — Common Lisp evaluates `&optional` \
         defaults on entry to each call, so the same shape is correct there.",
    )
    .with_example(
        "(defn collect [item [acc []]]\n  (.append acc item)\n  acc)",
        "(defn collect [item [acc None]]\n  (when (is acc None)\n    (setv acc []))\n  \
         (.append acc item)\n  acc)",
    )
    .with_caveat(
        "`#(…)` is a tuple and `\"…\"`, numbers and `None` are immutable, so none of them is \
         reported. A default that is an arbitrary call — `(default-timeout)` — is also evaluated \
         once, but sharing its result is usually intended, so only the builtin mutable \
         constructors (`list`, `dict`, `set`, `bytearray`, `defaultdict`, `Counter`, \
         `OrderedDict`, `deque`) are reported.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defn"),
    NormalizedHead::new("fn"),
    NormalizedHead::new("defn/a"),
    NormalizedHead::new("fn/a"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        // `DIALECTS` is the domain's own, so the scope and the analysis cannot
        // drift apart. The default is `COMMON_LISP_ONLY`, which for this rule
        // would mean running on the one dialect where the shape is *correct*
        // and never on the one where it is a bug.
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // Load-bearing beyond the guard: `defn` takes a name before its
        // parameter list and `fn` does not, so `parameter_list` needs to know
        // which head this is.
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        let Some((index, parameters)) = parameter_list(view, head) else {
            return Ok(());
        };
        let pairs = default_pairs(parameters);
        if pairs.is_empty() {
            return Ok(());
        }
        let body = view.children.get(index + 1..).unwrap_or(&[]);

        let mut reported = Vec::new();
        for pair in pairs {
            let value = &pair.children[1];
            if !is_mutable_default(value) {
                continue;
            }
            // A default nothing mutates is shared harmlessly. Reporting the
            // shape alone produced 88 findings over 2355 third-party files,
            // most of them read-only lookup tables like `[notes [0 5 7]]`.
            let Some(name) = atom_text(&pair.children[0]) else {
                continue;
            };
            if !parameter_is_mutated(body, name) {
                continue;
            }
            reported.push((pair, value));
        }
        if reported.is_empty() {
            return Ok(());
        }
        // Only now, with a finding otherwise ready: this reads the root view.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        for (pair, value) in reported {
            let name = pair
                .children
                .first()
                .and_then(atom_text)
                .unwrap_or("the parameter");
            sink.report(
                value.span,
                format!(
                    "{name} defaults to a fresh mutable object, which Hy evaluates once at \
                     definition time so every call shares it; default to None and build it in \
                     the body"
                ),
            );
        }
        Ok(())
    }
}
