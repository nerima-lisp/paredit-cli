//! What each rule here costs when it is handed thousands of its own heads —
//! and the measurement that kept a third rule out of the package.
//!
//! **Both shipped rules are local to the form the dispatcher hands them.** That
//! is a result of this file rather than a starting assumption.
//!
//! # The rule this file rejected
//!
//! `compiler-macro-disagrees-with-function` was designed and then **not built**.
//! Its premise is sound and was confirmed against SBCL 2.6.0: an incongruent
//! `define-compiler-macro` is a *silent* dead optimization —
//!
//! ```text
//! defun f2 (a b) + compiler-macro f2 (a)  [incongruent] => OK value=3
//!   warnings=SIMPLE-WARNING: Error during compiler-macroexpansion of (F2 1 2).
//! defun f3 (a b) + compiler-macro f3 (a b) [congruent control] => OK value=3
//! ```
//!
//! — the program still answers 3, the compiler macro simply never applies, and
//! the only diagnostic is at the *call site*, which may be in another file or
//! may not exist yet.
//!
//! But it is a **correlation**: `define-compiler-macro f` is only diagnosable
//! against the `defun f` elsewhere in the file, and `HeadFilter::WholeTree` is
//! not available to this package. That leaves a top-level scan per matched
//! head, which is the shape two rules have already been dropped on in this
//! repository. Rather than build it and measure it, the *shape* is measured
//! here directly, as [`TopLevelCorrelationRule`] — an allocation-free scan over
//! `SyntaxTree::root_child_span`, which is the cheapest spelling this
//! repository has for it. Release build, `dense_definition_source`, load
//! average 39:
//!
//! ```text
//!    clean cost-control-top-level-correlation  8x ratio = 170  ([3467246, 12998582, 69615813, 589786962] ns)
//!    dense cost-control-top-level-correlation  8x ratio = 179  ([1241204, 10592847, 62334512, 223387102] ns)
//!    clean macro-body-destroys-argument-form   8x ratio =  19  ([ 704845,  1454391,  4727173,  13713587] ns)
//!    clean macrolet-expander-captures-…        8x ratio =  15  ([ 259706,   563924,  1600327,   3905394] ns)
//!    clean cost-control-noop                   8x ratio =  10  ([  22451,    53754,   105666,    232129] ns)
//! ```
//!
//! **0.59 seconds for one rule on one file at n=2000**, at a doubling ratio of
//! 170 where linear is 8 — 43x the shipped rules' absolute cost and 17x the
//! no-op's ratio. The rule was not built.
//!
//! # On the residual super-linearity of the shipped rules
//!
//! Stated plainly rather than buried: **both shipped rules are super-linear**,
//! at ratios of 15–19 against the no-op's 10. The cause is
//! `crate::support::child_containing`, which scans a node's siblings in source
//! order, so the i-th top-level form costs i steps to reach. That is O(n²) in
//! the number of top-level forms, with a very small constant — the same shape,
//! by construction, as the shipped
//! `paredit-feature-lint-data-structure::support::locate`, which is this
//! repository's accepted spelling for a root descent.
//!
//! It is a different order of magnitude from the correlation control, whose
//! quadratic factor is *per invocation* rather than per descent, and it only
//! applies to a form that already has a candidate. But it is real, and a file
//! with tens of thousands of top-level macro definitions would feel it.
//!
//! ```text
//! cargo test -p paredit-feature-lint-macro-authoring --lib cost_ -- --nocapture
//! cargo test -p paredit-feature-lint-macro-authoring -- --ignored --nocapture ignored_bench_
//! ```
//!
//! # The controls
//!
//! - **`cost-control-noop`** declares the same heads and the same dialect scope
//!   and does nothing, so the difference between its column and a real rule's is
//!   that rule's own work rather than the dispatcher's.
//! - **`cost-control-top-level-correlation`** is the shape the rejected rule
//!   would have had.
//! - **the two real rules**, whose ratios must stay next to the no-op's.
//!
//! # What runs unattended
//!
//! Only the **invocation counts** and the cheap/expensive *ordering* assertion.
//! They are decided by the head index and by control flow, not by the clock.
//! The **doubling ratios are benchmarks, not tests**, and are `#[ignore]`d: a
//! ratio between two wall-clock durations is unstable under parallel load at
//! any threshold, and the machine these were taken on reported a load average
//! of **39** at the time. Read the ratios, not the durations.

use std::path::Path;
use std::time::Duration;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{
    PassOptions, RuleContext, RuleSink, build_head_index, collect_lint_pass,
};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
use paredit_core_lint_engine::rule::{LintRule, RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

// -- the controls -------------------------------------------------------------

/// The heads both shipped rules together declare.
const ALL_HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
    NormalizedHead::new("macrolet"),
];

#[derive(Debug)]
struct NoopRule;

const NOOP_META: RuleMeta = RuleMeta::new(
    "cost-control-noop",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a control that matches the same heads and does nothing",
    Fixability::ReportOnly,
);

static NOOP_RULE: NoopRule = NoopRule;

impl LintRule for NoopRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&ALL_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        _view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        Ok(())
    }
}

/// The shape `compiler-macro-disagrees-with-function` would have needed: for
/// each `define-compiler-macro`, find the `defun` of the same name among the
/// file's top-level forms.
///
/// Written the cheapest way this repository knows — `root_child_span` plus a
/// source-text read, allocating nothing per sibling — so the number it produces
/// is a *lower bound* on what the rule would have cost.
#[derive(Debug)]
struct TopLevelCorrelationRule;

const CORRELATION_META: RuleMeta = RuleMeta::new(
    "cost-control-top-level-correlation",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a control with the top-level correlation scan a compiler-macro congruence rule would need",
    Fixability::ReportOnly,
);

static CORRELATION_RULE: TopLevelCorrelationRule = TopLevelCorrelationRule;

const COMPILER_MACRO_HEAD: [NormalizedHead; 1] = [NormalizedHead::new("define-compiler-macro")];

impl LintRule for TopLevelCorrelationRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&COMPILER_MACRO_HEAD)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let tree = context.tree();
        let source = tree.source();
        let mut defuns = 0usize;
        for index in 0..tree.root_children().len() {
            let Some(span) = tree.root_child_span(index) else {
                continue;
            };
            let Some(text) = source.get(span.start().get()..span.end().get()) else {
                continue;
            };
            if text
                .trim_start_matches('(')
                .trim_start()
                .get(..5)
                .is_some_and(|head| head.eq_ignore_ascii_case("defun"))
            {
                defuns += 1;
            }
        }
        assert!(defuns < usize::MAX);
        Ok(())
    }
}

static ENTRIES: [RuleEntry; 4] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(&CORRELATION_META, &CORRELATION_RULE),
    RuleEntry::new(
        &crate::macro_body_destroys_argument_form::rule::META,
        &crate::macro_body_destroys_argument_form::rule::RULE,
    ),
    RuleEntry::new(
        &crate::macrolet_expander_captures_lexical_variable::rule::META,
        &crate::macrolet_expander_captures_lexical_variable::rule::RULE,
    ),
];

const COLUMNS: [&str; 4] = [
    "macro-body-destroys-argument-form",
    "macrolet-expander-captures-lexical-variable",
    "cost-control-top-level-correlation",
    "cost-control-noop",
];

// -- measurement --------------------------------------------------------------

fn measure_as(source: &str, dialect: Dialect) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("measured lint pass");
    outcome
        .timings
        .expect("measure: true produces timings")
        .entries()
        .map(|(position, elapsed, invocations)| {
            (
                catalog.entries()[position].meta().name().as_str(),
                elapsed,
                invocations,
            )
        })
        .collect()
}

fn measure(source: &str) -> Vec<(&'static str, Duration, u64)> {
    measure_as(source, Dialect::CommonLisp)
}

fn nanos_of(rows: &[(&'static str, Duration, u64)], rule: &str) -> u128 {
    rows.iter()
        .find(|(name, _, _)| *name == rule)
        .map(|(_, elapsed, _)| elapsed.as_nanos())
        .expect("the rule is in the catalogue")
}

fn invocations_of(rows: &[(&'static str, Duration, u64)], rule: &str) -> u64 {
    rows.iter()
        .find(|(name, _, _)| *name == rule)
        .map(|(_, _, invocations)| *invocations)
        .expect("the rule is in the catalogue")
}

// -- fixtures -----------------------------------------------------------------

/// `count` **correct** macro definitions — the zero-finding shape the
/// `clean/forms/*` benchmarks measure. Every rule must decline all of them, and
/// the cost of declining is what these numbers are.
///
/// Deliberately dense in the things that make these rules work: a `&body`
/// parameter, a `nreverse` of a *local* accumulator, a template-only
/// `macrolet` inside a `let`, and a `define-compiler-macro`.
fn clean_source(count: usize) -> String {
    let mut out = String::from(";;; correct macro authoring throughout\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defmacro with-thing{index} (name &body body)\n\
             \x20 (let ((out '()))\n\
             \x20   (dolist (form body) (push form out))\n\
             \x20   `(let ((,name (open-thing)))\n\
             \x20      (unwind-protect (progn ,@(nreverse out)) (close-thing ,name)))))\n\
             (define-compiler-macro scale{index} (a b) `(* ,a ,b))\n\
             (defun scale{index} (a b) (* a b))\n\
             (defun emit{index} (items)\n\
             \x20 (let ((code '()))\n\
             \x20   (macrolet ((emit (op) `(push ,op code)))\n\
             \x20     (dolist (item items) (emit item))\n\
             \x20     (nreverse code))))\n"
        ));
    }
    out
}

/// The reporting shape for `macro-body-destroys-argument-form`: every macro
/// destroys its `&body`.
///
/// The fixtures that *report* are what expose a per-finding cost. A rule that
/// is cheap while declining and expensive while reporting is exactly the shape
/// `is_unevaluated_at` has, and it is invisible to the clean fixture.
fn destructive_heavy_source(count: usize) -> String {
    (0..count)
        .map(|index| format!("(defmacro bad{index} (&body forms) `(progn ,@(nreverse forms)))\n"))
        .collect()
}

/// The reporting shape for `macrolet-expander-captures-lexical-variable`,
/// nested so the root descent has real depth to walk.
fn capturing_heavy_source(count: usize) -> String {
    (0..count)
        .map(|index| {
            format!(
                "(defun host{index} (n)\n\
                 \x20 (let ((limit n))\n\
                 \x20   (macrolet ((rep{index} () limit)) (rep{index}))))\n"
            )
        })
        .collect()
}

/// Many `define-compiler-macro`s **and** many top-level forms in one file: the
/// worst case for a correlation, where both factors grow together.
fn dense_definition_source(count: usize) -> String {
    let mut out = String::new();
    for index in 0..count {
        out.push_str(&format!(
            "(define-compiler-macro g{index} (a b) `(+ ,a ,b))\n"
        ));
    }
    for index in 0..count {
        out.push_str(&format!("(defun g{index} (a b) (+ a b))\n"));
    }
    out
}

/// The 8x range the doubling ratio is read over, used by the benchmarks alone.
const SIZES: [usize; 4] = [250, 500, 1000, 2000];

/// The sizes the *unattended* tests use. Deliberately small: what they assert
/// is an invocation count, which the head index decides before any `check` body
/// runs and which is therefore the same number at 100 as at 2000.
const UNATTENDED_SIZES: [usize; 2] = [100, 250];

fn node_count(source: &str) -> u64 {
    fn walk(view: &ExpressionView) -> u64 {
        1 + view.children.iter().map(walk).sum::<u64>()
    }
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    tree.root_view().children.iter().map(walk).sum()
}

fn report(label: &str, rows: &[(&'static str, Duration, u64)], size: usize) {
    for rule in COLUMNS {
        let invocations = invocations_of(rows, rule);
        let nanos = nanos_of(rows, rule);
        let per = if invocations == 0 {
            0
        } else {
            nanos / u128::from(invocations)
        };
        println!(
            "{label:>10} n={size:<5} {rule:<48} {:>9}us  invocations={invocations:<6} \
             {per:>9}ns/inv",
            nanos / 1000
        );
    }
}

fn ratio(small: u128, large: u128) -> u128 {
    large / small.max(1)
}

// -- what runs unattended -----------------------------------------------------

/// The head index must hand each rule one node per *form it declares a head
/// for*, and no more.
///
/// The node count is the control. Without it, "N invocations at n=N" is also
/// what a per-node dispatch would report on a file that happened to have N
/// nodes.
#[test]
fn cost_each_rule_is_dispatched_once_per_head_not_per_node() {
    for size in UNATTENDED_SIZES {
        let source = clean_source(size);
        let nodes = node_count(&source);
        let rows = measure(&source);
        report("clean", &rows, size);

        assert!(
            nodes > (size as u64) * 20,
            "the fixture has {nodes} nodes for {size} groups; a per-head count cannot be told \
             apart from a per-node one"
        );
        assert_eq!(
            invocations_of(&rows, "macro-body-destroys-argument-form"),
            (size as u64) * 2,
            "one per defmacro plus one per define-compiler-macro, not per node ({nodes} nodes)"
        );
        assert_eq!(
            invocations_of(&rows, "macrolet-expander-captures-lexical-variable"),
            size as u64,
            "one per macrolet"
        );
        assert_eq!(
            invocations_of(&rows, "cost-control-top-level-correlation"),
            size as u64,
            "one per define-compiler-macro"
        );
        assert_eq!(
            crate::engine_pass_tests::fired_names(&source, Dialect::CommonLisp),
            Vec::<&str>::new(),
            "the clean cost fixture is not clean, so these numbers are the cost of reporting"
        );
    }
}

/// **The correlation shape costs orders of magnitude more than a local check.**
///
/// This is the measurement that kept `compiler-macro-disagrees-with-function`
/// out of the package, and it is stated as a ratio against `cost-control-noop`
/// in the same pass rather than as an absolute time, because a ratio survives a
/// loaded machine where a duration does not.
///
/// The fixture is [`dense_definition_source`], where the number of compiler
/// macros and the number of top-level forms grow together — the shape the rule
/// would actually have met, since a file defining compiler macros defines the
/// functions too.
#[test]
fn cost_the_rejected_correlation_shape_is_separable_from_a_local_check() {
    let rows = measure(&dense_definition_source(250));
    report("dense", &rows, 250);
    let noop = nanos_of(&rows, "cost-control-noop").max(1);
    let correlation = nanos_of(&rows, "cost-control-top-level-correlation");
    assert_eq!(
        invocations_of(&rows, "cost-control-top-level-correlation"),
        250,
        "the fixture must offer the correlation every one of its 250 compiler macros"
    );
    assert!(
        correlation > noop * 20,
        "the correlation control cost {correlation}ns against the no-op's {noop}ns; if a \
         whole-file scan is not separable from a no-op on this machine, this file's central \
         claim cannot be read from it"
    );
    // The shipped rule that sees the same heads must not be in that regime.
    let local = nanos_of(&rows, "macro-body-destroys-argument-form");
    assert!(
        local < correlation,
        "macro-body-destroys-argument-form cost {local}ns, at or above the {correlation}ns \
         whole-file scan it must not resemble"
    );
}

/// A file with none of these heads must not reach a single `check` body. This
/// is what the CI `bench-compare` gate measures, and the reason every rule here
/// declares `HeadFilter::Heads` rather than `WholeTree`.
#[test]
fn cost_a_file_without_these_heads_never_reaches_a_check() {
    let source: String = (0..4000)
        .map(|index| format!("(defun f{index} (a b) (+ a b {index}))\n"))
        .collect();
    let rows = measure(&source);
    for rule in COLUMNS {
        assert_eq!(
            invocations_of(&rows, rule),
            0,
            "{rule} was invoked on a file with none of its heads"
        );
    }
}

/// A rule must not be dispatched at all for a dialect it does not model.
#[test]
fn cost_is_zero_for_a_dialect_no_rule_here_models() {
    let source: String = (0..2000)
        .map(|index| format!("(defmacro m{index} [a b] (+ a b))\n"))
        .collect();
    for (name, elapsed, invocations) in measure_as(&source, Dialect::Clojure) {
        assert_eq!(invocations, 0, "{name} ran for Clojure");
        assert_eq!(elapsed, Duration::ZERO, "{name} was timed for Clojure");
    }
}

/// A rule that *reports* on every form is still dispatched once per form, and
/// the reporting path must stay bounded.
#[test]
fn cost_a_reporting_rule_is_still_dispatched_once_per_form() {
    for size in UNATTENDED_SIZES {
        let rows = measure(&destructive_heavy_source(size));
        report("destruct", &rows, size);
        assert_eq!(
            invocations_of(&rows, "macro-body-destroys-argument-form"),
            size as u64,
            "one per defmacro even when every one of them reports"
        );

        let rows = measure(&capturing_heavy_source(size));
        report("capture", &rows, size);
        assert_eq!(
            invocations_of(&rows, "macrolet-expander-captures-lexical-variable"),
            size as u64,
            "one per macrolet even when every one of them reports"
        );
    }
}

// -- benchmarks ---------------------------------------------------------------

/// The doubling ratio, over an 8x range in [`SIZES`].
///
/// Linear work gives ~8. The correlation control is O(compiler-macros x
/// top-level forms) and is **expected to give ~64** on
/// [`dense_definition_source`] — that is the shape of a pairing, and it is
/// printed rather than asserted so it can be read. Both shipped rules must sit
/// near 8 on every fixture, including the ones where they report on every form.
#[test]
#[ignore = "a benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_doubling_ratio() {
    for (label, generate) in [
        ("clean", clean_source as fn(usize) -> String),
        ("dense", dense_definition_source as fn(usize) -> String),
        ("destruct", destructive_heavy_source as fn(usize) -> String),
        ("capture", capturing_heavy_source as fn(usize) -> String),
    ] {
        let mut columns: Vec<Vec<u128>> = vec![Vec::new(); COLUMNS.len()];
        for size in SIZES {
            let rows = measure(&generate(size));
            report(label, &rows, size);
            for (slot, rule) in COLUMNS.iter().enumerate() {
                columns[slot].push(nanos_of(&rows, rule));
            }
        }
        for (slot, rule) in COLUMNS.iter().enumerate() {
            let nanos = &columns[slot];
            println!(
                "{label:>10} {rule:<48} 8x ratio = {:>4}   ({nanos:?} ns over {SIZES:?}) \
                 -- linear is ~8, an O(m*n) pairing is ~64",
                ratio(nanos[0], nanos[3])
            );
        }
    }
}
