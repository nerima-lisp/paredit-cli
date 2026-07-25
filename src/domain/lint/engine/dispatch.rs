//! The single pre-order pass that runs every active rule.

use std::path::Path;

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::list_head;

use super::context::RuleContext;
use super::head_index::{head_index, head_key};
use super::ordering::{RuleIndex, VisitIndex};
use super::sink::FindingSink;
use crate::domain::lint::model::LintOutcome;
use crate::domain::lint::policy::RuleSelection;
use crate::domain::lint::registry::{REGISTRY, RULE_COUNT};

/// The rules that will actually run, decided once before the walk.
///
/// Both filters — the caller's selection and each rule's dialect scope — are
/// answered up front so the inner loop is an array index rather than a string
/// comparison against the active list for every node.
#[derive(Debug)]
struct ActiveRules {
    enabled: [bool; RULE_COUNT],
    any: bool,
}

impl ActiveRules {
    fn resolve(dialect: Dialect, selection: RuleSelection<'_>) -> Self {
        let mut enabled = [false; RULE_COUNT];
        let mut any = false;
        for (position, entry) in REGISTRY.iter().enumerate() {
            let active = selection.includes(entry.meta().name())
                && entry.rule().dialect_scope().includes(dialect);
            enabled[position] = active;
            any |= active;
        }
        Self { enabled, any }
    }

    fn contains(&self, rule: RuleIndex) -> bool {
        self.enabled[rule.get()]
    }
}

/// Runs every selected rule over one parsed file and returns each finding with
/// the fix its rule can apply, in the report's canonical order.
///
/// One pre-order walk serves all of them: head-specific rules are reached
/// through the operator index, shape rules see every node, and the few rules
/// that correlate separate definitions get the document once before the walk.
pub fn collect_lint_outcomes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    selection: RuleSelection<'_>,
) -> Result<Vec<LintOutcome>> {
    let active = ActiveRules::resolve(dialect, selection);
    if !active.any {
        return Ok(Vec::new());
    }

    let context = RuleContext::new(path, dialect, tree, source);
    let index = head_index();
    let mut sink = FindingSink::new(path);

    let root = tree.root_view();
    for rule in index.whole_tree() {
        if active.contains(*rule) {
            check(&context, *rule, VisitIndex::ROOT, &root, &mut sink)?;
        }
    }

    let mut visit = VisitIndex::ROOT;
    for child in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(child))?.view();
        walk(&context, &active, &view, &mut visit, &mut sink)?;
    }

    Ok(sink.into_ordered())
}

/// Pre-order, iteratively: a deeply nested document must not depend on stack
/// depth, and pre-order is exactly what the per-rule walks it replaces
/// produced.
fn walk(
    context: &RuleContext<'_>,
    active: &ActiveRules,
    root: &ExpressionView,
    visit: &mut VisitIndex,
    sink: &mut FindingSink<'_>,
) -> Result<()> {
    let index = head_index();
    let mut stack = vec![root];

    while let Some(view) = stack.pop() {
        *visit = visit.next();
        let position = *visit;

        for rule in index.all_nodes() {
            if active.contains(*rule) {
                check(context, *rule, position, view, sink)?;
            }
        }

        if let Some(head) = list_head(view) {
            let key = head_key(context.dialect(), head);
            for rule in index.for_head(&key) {
                if active.contains(*rule) {
                    check(context, *rule, position, view, sink)?;
                }
            }
        }

        stack.extend(view.children.iter().rev());
    }

    Ok(())
}

fn check(
    context: &RuleContext<'_>,
    rule: RuleIndex,
    visit: VisitIndex,
    view: &ExpressionView,
    sink: &mut FindingSink<'_>,
) -> Result<()> {
    let entry = &REGISTRY[rule.get()];
    let mut scoped = sink.visiting(rule, entry.meta().name(), visit);
    entry.rule().check(context, view, &mut scoped)
}
