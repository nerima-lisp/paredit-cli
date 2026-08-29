//! `elisp-obsolete-cl-alias`: a `cl.el` name that Emacs 27 removed.
//!
//! `cl.el` provided unprefixed aliases — `loop`, `case`, `flet`, `incf` — and
//! was deleted in Emacs 27.1. A file still using them does not merely warn; it
//! fails to load, because the symbols no longer exist.
//!
//! Most of the renames are mechanical: `loop` became `cl-loop`, `case` became
//! `cl-case`, with identical semantics. Two are not, and the message says so:
//! `flet` and `labels` rebound the *function cell* for a dynamic extent, while
//! `cl-flet` and `cl-labels` create lexical bindings. Code that relied on the
//! old behaviour to stub out a function seen by a callee needs `cl-letf`
//! instead, which is why this rule reports rather than fixes.
//!
//! Every one of these names is also an ordinary variable name, so the head is
//! not on its own evidence of a call. A sweep of GNU Emacs 31 found the rule
//! reporting `(dolist (block blocks) …)`, `(let (ll (do t)) …)`,
//! `(mapcar (lambda (case) …) …)` and `(defun mail-comma-list-regexp (labels)
//! …)` — binding pairs and lambda lists, none of them evaluated as a call.
//! `could_be_a_call` asks the two questions that can be answered from the
//! node alone: does the form have an operand *and* a body, and does its first
//! argument have the shape the macro's lambda list required.
//!
//! What cannot be answered from the node alone is whether the form sits in
//! operator position at all. `(dolist (block blocks) …)` is caught here only
//! because a one-argument `block` is not a call; a binding pair that happens
//! to have a callable shape still gets through, and would need the parent that
//! `RuleContext` does not carry.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use crate::shared::list_head;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-obsolete-cl-alias",
    RuleCategory::Suspicious,
    Severity::Error,
    "an unprefixed cl.el name, removed in Emacs 27.1",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 14] = [
    NormalizedHead::new("block"),
    NormalizedHead::new("case"),
    NormalizedHead::new("destructuring-bind"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
    NormalizedHead::new("ecase"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("letf"),
    NormalizedHead::new("letf*"),
    NormalizedHead::new("loop"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("multiple-value-bind"),
    NormalizedHead::new("symbol-macrolet"),
];

/// Whether the rename also changes what the form means.
fn rebinds_the_function_cell(head: &str) -> bool {
    matches!(head, "flet" | "labels")
}

/// What each removed macro requires of its first argument.
///
/// These names are also perfectly ordinary *variable* names, and a `(…)` list
/// headed by one is far more often a binding pair or a lambda list than a
/// call. GNU Emacs writes `(dolist (block blocks) …)`, `(let (ll (do t)) …)`,
/// `(mapcar (lambda (case) …) …)` and
/// `(defun mail-comma-list-regexp (labels) …)` — none of which is a call to
/// anything. The lambda list of the macro each name used to have is what tells
/// the two apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstArgument {
    /// A binding list, variable list or arglist — always a `(…)` list.
    List,
    /// A block name — always a symbol.
    Symbol,
    /// An arbitrary expression: `case` and `ecase` dispatch on one, and a
    /// `loop` clause can open with either a keyword or a form. Unconstrained.
    Anything,
}

fn first_argument_of(head: &str) -> FirstArgument {
    match head {
        "destructuring-bind"
        | "do"
        | "do*"
        | "flet"
        | "labels"
        | "letf"
        | "letf*"
        | "macrolet"
        | "multiple-value-bind"
        | "symbol-macrolet" => FirstArgument::List,
        "block" => FirstArgument::Symbol,
        _ => FirstArgument::Anything,
    }
}

/// Whether `view` could be a call to the macro `head` names.
///
/// Two things are asked. First, that there are at least two arguments: every
/// one of these macros takes an operand *and* a body, so `(block blocks)` and
/// `(labels)` are not degenerate calls, they are a `dolist` binding pair and a
/// parameter list. Second, that the first argument has the shape the macro's
/// lambda list requires.
fn could_be_a_call(view: &ExpressionView, head: &str) -> bool {
    // The head plus two arguments.
    if view.children.len() < 3 {
        return false;
    }
    let Some(first) = view.children.get(1) else {
        return false;
    };
    match first_argument_of(head) {
        FirstArgument::List => first.kind == ExpressionKind::List,
        FirstArgument::Symbol => first.kind == ExpressionKind::Atom,
        FirstArgument::Anything => true,
    }
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        // The engine matched a *normalized* head, which lowercases; Emacs Lisp
        // does not, so `LOOP` is a name a package may define.
        if !HEADS.iter().any(|known| known.as_str() == head) {
            return Ok(());
        }
        // `'(do doing)` is a symbol list a `memq` searches, and
        // `'(do you know Stallman \?)` is a sentence `doctor.el` types at you.
        // Neither is evaluated, so neither can fail to load.
        if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
            return Ok(());
        }
        if !could_be_a_call(view, head) {
            return Ok(());
        }
        // Last, and only once every cheap structural check has passed: this
        // builds the whole file's binding table.
        //
        // `(named-let loop ((n d)) … (loop (cdr n)))` binds `loop` as a local
        // function, so the call in the body is a call to *that*, not to the
        // macro Emacs 27 deleted. GNU Emacs uses the idiom throughout
        // `byte-opt.el`, `bytecomp.el`, `package.el` and `oclosure.el`, and it
        // was the single largest source of false reports here. A head that
        // resolves to any binding in scope is not the removed macro.
        let resolves_to_a_local_binding = view
            .children
            .first()
            .and_then(NodeKey::of)
            .and_then(|key| context.binding_table().resolve(key))
            .is_some();
        if resolves_to_a_local_binding {
            return Ok(());
        }

        let message = if rebinds_the_function_cell(head) {
            format!(
                "`{head}` was removed in Emacs 27.1; `cl-{head}` replaces it \
                 but binds lexically rather than rebinding the function cell — \
                 use `cl-letf` if a callee must see the replacement"
            )
        } else {
            format!("`{head}` was removed in Emacs 27.1; use `cl-{head}`")
        };

        let span = view.children.first().map_or(view.span, |child| child.span);
        sink.report(span, message);
        Ok(())
    }
}
