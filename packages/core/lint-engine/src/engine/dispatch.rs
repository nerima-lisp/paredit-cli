//! The single pre-order pass that runs every active rule.

use std::path::Path;
use std::time::Instant;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

use super::context::RuleContext;
use super::head_index::{HeadIndex, head_key};
use super::ordering::{RuleIndex, VisitIndex};
use super::sink::FindingSink;
use super::timings::RuleTimings;
use crate::error::LintResult;
use crate::model::{LintOutcome, RuleSettings};
use crate::policy::RuleSelection;
use crate::rule::RuleCatalog;

/// The rules that will actually run, decided once before the walk.
///
/// Both filters — the caller's selection and each rule's dialect scope — are
/// answered up front so the inner loop is an array index rather than a string
/// comparison against the active list for every node.
#[derive(Debug)]
struct ActiveRules {
    // Sized from the catalogue rather than a `RULE_COUNT` const, which would
    // make this type's size depend on how many rules exist. Built once per
    // file, never per node, so the allocation is not on the hot path - and
    // `contains` stays an index either way.
    enabled: Box<[bool]>,
    any: bool,
}

impl ActiveRules {
    fn resolve(catalog: RuleCatalog, dialect: Dialect, selection: RuleSelection<'_>) -> Self {
        let mut enabled = vec![false; catalog.len()];
        let mut any = false;
        for (position, entry) in catalog.entries().iter().enumerate() {
            let active = selection.includes(entry.meta().name())
                && entry.rule().dialect_scope().includes(dialect);
            enabled[position] = active;
            any |= active;
        }
        Self {
            enabled: enabled.into_boxed_slice(),
            any,
        }
    }

    fn contains(&self, rule: RuleIndex) -> bool {
        self.enabled[rule.get()]
    }
}

/// Everything about a pass beyond "which catalogue, over which file".
///
/// A struct rather than two more parameters on an already seven-parameter
/// function: both are absent on almost every call, and a call site reading
/// `collect_lint_pass(c, i, p, d, t, s, sel, None, false)` says nothing about
/// which argument is which.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassOptions<'a> {
    /// The caller's `--rule-arg` overrides.
    pub settings: Option<&'a RuleSettings>,
    /// Whether to time each `check` call. Off by default: two clock reads per
    /// (rule, node) pair is a cost an untimed run must not pay.
    pub measure: bool,
}

/// What one pass produced.
#[derive(Debug)]
pub struct PassOutcome {
    pub outcomes: Vec<LintOutcome>,
    /// Present exactly when [`PassOptions::measure`] was set.
    pub timings: Option<RuleTimings>,
}

/// Runs every selected rule over one parsed file and returns each finding with
/// the fix its rule can apply, in the report's canonical order.
///
/// One pre-order walk serves all of them: head-specific rules are reached
/// through the operator index, shape rules see every node, and the few rules
/// that correlate separate definitions get the document once before the walk.
///
/// `catalog` and `index` are supplied by whoever owns the registry, so this
/// module never names a rule or counts them (section 4.2).
pub fn collect_lint_outcomes(
    catalog: RuleCatalog,
    index: &HeadIndex,
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    selection: RuleSelection<'_>,
) -> LintResult<Vec<LintOutcome>> {
    Ok(collect_lint_pass(
        catalog,
        index,
        path,
        dialect,
        tree,
        source,
        selection,
        PassOptions::default(),
    )?
    .outcomes)
}

/// [`collect_lint_outcomes`] with the per-run knobs: rule settings, and
/// optional per-rule timing.
#[allow(clippy::too_many_arguments)]
pub fn collect_lint_pass(
    catalog: RuleCatalog,
    index: &HeadIndex,
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    selection: RuleSelection<'_>,
    options: PassOptions<'_>,
) -> LintResult<PassOutcome> {
    let active = ActiveRules::resolve(catalog, dialect, selection);
    let mut timings = options.measure.then(|| RuleTimings::new(catalog.len()));
    if !active.any {
        return Ok(PassOutcome {
            outcomes: Vec::new(),
            timings,
        });
    }

    let context = RuleContext::new(path, dialect, tree, source);
    let context = match options.settings {
        Some(settings) => context.with_settings(settings),
        None => context,
    };
    let mut sink = FindingSink::new(path);

    let root = tree.root_view();
    for rule in index.whole_tree() {
        if active.contains(*rule) {
            check(
                catalog,
                &context,
                *rule,
                VisitIndex::ROOT,
                &root,
                &mut sink,
                timings.as_mut(),
            )?;
        }
    }

    let mut visit = VisitIndex::ROOT;
    for child in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(child))?.view();
        walk(
            catalog,
            index,
            &context,
            &active,
            &view,
            &mut visit,
            &mut sink,
            timings.as_mut(),
        )?;
    }

    Ok(PassOutcome {
        outcomes: sink.into_ordered(),
        timings,
    })
}

/// Pre-order, iteratively: a deeply nested document must not depend on stack
/// depth, and pre-order is exactly what the per-rule walks it replaces
/// produced.
#[allow(clippy::too_many_arguments)]
fn walk(
    catalog: RuleCatalog,
    index: &HeadIndex,
    context: &RuleContext<'_>,
    active: &ActiveRules,
    root: &ExpressionView,
    visit: &mut VisitIndex,
    sink: &mut FindingSink<'_>,
    mut timings: Option<&mut RuleTimings>,
) -> LintResult {
    let mut stack = vec![root];

    while let Some(view) = stack.pop() {
        *visit = visit.next();
        let position = *visit;

        for rule in index.all_nodes() {
            if active.contains(*rule) {
                check(
                    catalog,
                    context,
                    *rule,
                    position,
                    view,
                    sink,
                    timings.as_deref_mut(),
                )?;
            }
        }

        if let Some(head) = list_head(view) {
            let key = head_key(context.dialect(), head);
            for rule in index.for_head(&key) {
                if active.contains(*rule) {
                    check(
                        catalog,
                        context,
                        *rule,
                        position,
                        view,
                        sink,
                        timings.as_deref_mut(),
                    )?;
                }
            }
        }

        stack.extend(view.children.iter().rev());
    }

    Ok(())
}

fn check(
    catalog: RuleCatalog,
    context: &RuleContext<'_>,
    rule: RuleIndex,
    visit: VisitIndex,
    view: &ExpressionView,
    sink: &mut FindingSink<'_>,
    timings: Option<&mut RuleTimings>,
) -> LintResult {
    let entry = &catalog.entries()[rule.get()];
    let Some(timings) = timings else {
        let mut scoped = sink.visiting(rule, entry.meta().name(), visit);
        return entry.rule().check(context, view, &mut scoped);
    };
    // The clock reads bracket only the rule's own work, so a rule that builds
    // the binding table on first use is charged for building it — which is the
    // number `--timings` exists to show.
    let started = Instant::now();
    let result = {
        let mut scoped = sink.visiting(rule, entry.meta().name(), visit);
        entry.rule().check(context, view, &mut scoped)
    };
    timings.record(rule, started.elapsed());
    result
}
