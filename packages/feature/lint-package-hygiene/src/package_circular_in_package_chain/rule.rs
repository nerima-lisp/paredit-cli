//! `package-circular-in-package-chain`: a top-level `in-package` that re-enters
//! a package the file had already left.
//!
//! The analysis lives in
//! [`crate::package_circular_in_package_chain::domain`], which also backs the
//! standalone `inspect package-circular-in-package-chain` command; this module
//! only registers it with the lint suite and phrases its findings.
//!
//! # Why this is `Heads` and not `WholeTree`
//!
//! The chain *is* a property of the file, which is the shape
//! `paredit-feature-lint-build-system`'s `defpackage-without-in-package` takes
//! the `WholeTree` exception for — and the exception was still declined here,
//! for the reason that decided that one: cost on the files the rule is *not*
//! about.
//!
//! `defpackage-without-in-package` is about an **absence**. There is no node to
//! anchor on, so `Heads` cannot express it at all, and its `WholeTree` root view
//! is free because the dispatcher materializes one for every file anyway.
//!
//! This rule is about a form that is **present**, and it reports *at* that form.
//! `Heads(&["in-package"])` therefore costs nothing on a file with no
//! `in-package` — the head index simply never routes to it, so `check` is not
//! called. `WholeTree` would instead be dispatched on every file and pay a byte
//! scan over each one, including all of `clean/forms/*`, which contain no
//! `in-package` and are exactly what the 10% `bench-compare` gate measures.
//!
//! # What one invocation costs, and why that is linear
//!
//! `check` is called once per `in-package` node. The rejection path is ordered
//! so that everything cheap happens first:
//!
//! 1. **Node-local shape.** `switched_package` reads the head and child 1 of
//!    the view the dispatcher already built. No tree access at all.
//! 2. **The ambient set.** `CL-USER` and friends are exempt, and this is the
//!    common `in-package` in a single-file library. A `Vec`-free `contains`
//!    over five `&str`.
//! 3. **The quote question.** `is_unevaluated_at` binary-searches the top level
//!    and materializes only the one top-level form containing the target — the
//!    enclosing form's size, never the file's. `SyntaxTree::root_view` is never
//!    called: it deep-materializes the whole document, uncached, on every call,
//!    and calling it here would make a file of T switches cost T×T.
//! 4. **Only then the top-level scan.** `top_level_in_package_chain` walks the
//!    document's root children through `select_path`, which resolves node ids
//!    and reads spans without allocating; it materializes an `ExpressionView`
//!    for no form at all, and slices heads and designators straight out of the
//!    source.
//!
//! Step 4 is linear in the number of top-level forms and is paid once per
//! `in-package` node. A file has a handful of top-level `in-package` forms — one
//! is the norm, and one alone can never produce a finding — so the total is a
//! small constant number of linear passes, which is linear in the file. The
//! `measurement` tests in `crate` pin that: the cost doubles when the file
//! doubles.
//!
//! No `RuleContext::binding_table`/`value_table`/`type_table` is consulted; this
//! rule needs no semantic pass. `RuleContext::scratch_cache` is not touched
//! either — it holds one type per file's pass and is already claimed by
//! `paredit-feature-lint-repl-debug`, which panics by construction if a second
//! type is stored during the same pass.

use paredit_core_lint_engine::LintResult;

use crate::package_circular_in_package_chain::domain::examine_in_package;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "package-circular-in-package-chain",
    // The file compiles and loads. What it means — which package each region's
    // unqualified symbols are interned into — is not what reading any one form
    // suggests. That is exactly `Suspicious`. Not `Duplicate`: the second
    // `in-package` is not a repeated key, it is a return to a place the file
    // had left. Not `Malformed`: every form here is well formed.
    RuleCategory::Suspicious,
    // A judgement about structure, not a settled defect: a file may split a
    // package's regions deliberately, and the code still works. `Warning` is
    // the honest level, and it keeps this out of the error budget of a project
    // that has decided it is fine.
    Severity::Warning,
    "a top-level in-package that re-enters a package the file had already left",
    // Merging two regions of one package means moving forms, and which region
    // should absorb the other is a decision a rewrite cannot make.
    Fixability::ReportOnly,
);

/// `examine_in_package` only ever matches an `in-package` head. `defpackage`
/// and `define-package` are deliberately *not* declared: this rule reports at
/// the switch, and declaring a head it never reports at would buy invocations
/// that can only return.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("in-package")];

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
        if let Some(item) = examine_in_package(context.tree(), view) {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
