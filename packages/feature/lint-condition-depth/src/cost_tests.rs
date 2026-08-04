//! What each rule here costs when it is handed thousands of its own heads —
//! and the measurement that kept a third rule out of the package.
//!
//! **Both shipped rules are local to the form the dispatcher hands them.** That
//! is a result of this file rather than a starting assumption.
//!
//! # The rule this file rejected
//!
//! `error-signalled-on-warning-condition` was designed, built, tested and then
//! **removed**. Its premise is sound and was confirmed against SBCL 2.6.0: with
//! `(define-condition my-warn (warning) ())`, `(error 'my-warn)` is legal, the
//! signalled object is `typep … 'warning` and **not** `typep … 'error`, so
//!
//! ```text
//! (ignore-errors (error 'my-warn))   => escaped ignore-errors!  TYPE=MY-WARN
//! (find-restart 'muffle-warning)     => NIL   under (error 'my-warn)
//!                                    => T     under (warn  'my-warn)
//! (compile nil '(lambda () (error 'my-warn)))  => warnings-p=NIL failure-p=NIL
//! ```
//!
//! — a call that looks guarded is not guarded, and the compiler says nothing.
//!
//! But deciding it needs the file's `define-condition` hierarchy, and a rule
//! gets no cache that outlives one `check()`: `RuleContext::scratch_cache` is a
//! single type-erased slot already claimed by
//! `paredit-feature-lint-repl-debug`, which panics by construction if a second
//! type is stored during the same pass. So the hierarchy is rebuilt per
//! invocation, and the rule is O(N²) in the number of `(error 'literal-sym …)`
//! calls. The shape is preserved below as [`FileHierarchyRule`] so the number
//! stays reproducible. Release build, load average 15–32, two runs:
//!
//! ```text
//! == clean: correct code, file defines a condition ==   (ZERO findings)
//!   cost-control-file-hierarchy    8x ratio = 126  [150350042 … 19079920060] ns
//!   cost-control-file-hierarchy    8x ratio =  54  [461983583 … 25070655496] ns  (rerun)
//!   condition-type-datum-…         8x ratio =  35  [    86643 …      3109055] ns
//!   unwind-protect-cleanup-signals 8x ratio =  39  [    74041 …      2896579] ns
//!   cost-control-shipped-local     8x ratio =  13  [    72559 …       963432] ns
//!   cost-control-noop              8x ratio =  18  [    30768 …       574640] ns
//!
//! == no-definition: the byte-scan guard path ==
//!   cost-control-file-hierarchy    8x ratio =  70  [  6538506 …    462754763] ns
//!
//! == warning-heavy: every error call reports ==
//!   cost-control-file-hierarchy    8x ratio = 112  [ 55559657 …   6244569640] ns
//! ```
//!
//! **19–25 seconds for one rule on one file at n=2000**, on a fixture with
//! **zero findings**, against a shipped rule's 0.96–1.12 ms in the same run:
//! four orders of magnitude, at a doubling ratio of 54–126 where linear is 8.
//! Three fixtures and two runs agree, so it is not the load average.
//!
//! Note the `no-definition` column especially. `support::mentions_define_condition`
//! reduces a hierarchy build to a byte scan for a file that defines no
//! condition — and it does not save the rule, because a byte scan is still
//! O(file) *per invocation*. The quadratic term survives its own optimization.
//! That is the finding, and it is why the rule is not shipped.
//!
//! # The controls
//!
//! - **`cost-control-noop`** declares the same heads and the same dialect scope
//!   and does nothing, so the difference between its column and a real rule's is
//!   that rule's own work rather than the dispatcher's.
//! - **`cost-control-file-hierarchy`** is the rejected rule, exactly as it was
//!   written, so its cost stays measurable next to the rules that shipped.
//! - **`cost-control-shipped-local`** reimplements the shape of
//!   `paredit-feature-lint-safety`'s **shipped** `handler-case-swallows-error`:
//!   a head-matched `Conditions` rule that reads a fixed number of children of
//!   its own node and never touches the tree. It is what "a shipped rule's cost"
//!   means in this package, measured in the same run.
//!
//!   A dev-dependency on the shipped package itself would be better, but
//!   `tests/cli/feature_dependency_contract.rs` scans the whole manifest text
//!   for `paredit-feature-` — `[dev-dependencies]` included — so a cross-feature
//!   dev dependency trips a contract this package is not allowed to edit. A
//!   local reimplementation of the shipped *shape* is what this repository
//!   permits, and it is spelled out below so the claim can be checked.
//!
//! # On `error-signalled-on-warning-condition`'s quadratic factor
//!
//! Stated plainly rather than buried. The rule builds a `LazyHierarchy` per
//! `check()` call and reads it only once the node is `(error 'literal-symbol …)`.
//! On a file with N such calls that is N hierarchy reads, so the rule is **O(N²)
//! in the number of literally-typed `error` calls**, with two things holding the
//! constant down:
//!
//! - a file that never spells `define-condition` pays a **byte scan** rather
//!   than a tree walk (`support::mentions_define_condition`), and
//! - a file that never spells `(error 'sym …)` pays **nothing at all**, because
//!   `named_condition_type` is two child lookups and fails first.
//!
//! The byte scan is still O(file) per invocation, so the quadratic term is real
//! in both fixtures below and is why both are measured. This is the same shape
//! as the shipped `signal-on-error-condition-returns-silently`, whose head
//! (`signal`) is rarer than this rule's (`error`) — so the exposure here is
//! genuinely larger than the precedent's, which is the reason for measuring
//! rather than assuming.
//!
//! ```text
//! cargo test -p paredit-feature-lint-condition-depth --lib cost_ -- --nocapture
//! cargo test -p paredit-feature-lint-condition-depth --lib -- --ignored --nocapture ignored_bench_
//! ```
//!
//! # What runs unattended
//!
//! Only the **invocation counts** and the ordering assertions that are decided
//! by the head index and by control flow rather than by the clock. The
//! **doubling ratios are benchmarks, not tests**, and are `#[ignore]`d: a ratio
//! between two wall-clock durations is unstable under parallel load at any
//! threshold, and this machine reported a load average above 20 while these were
//! taken. Read the ratios, not the durations.

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
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, symbol_in};

// -- the controls -------------------------------------------------------------

/// Every head the three rules together declare, so each control sees exactly the
/// invocations the real rules see.
const ALL_HEADS: [NormalizedHead; 6] = [
    NormalizedHead::new("error"),
    NormalizedHead::new("cerror"),
    NormalizedHead::new("signal"),
    NormalizedHead::new("warn"),
    NormalizedHead::new("make-condition"),
    NormalizedHead::new("unwind-protect"),
];

#[derive(Debug)]
struct NoopRule;

const NOOP_META: RuleMeta = RuleMeta::new(
    "cost-control-noop",
    RuleCategory::Conditions,
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
    ) -> LintResult {
        Ok(())
    }
}

/// The **shipped** local shape, reimplemented so it can be measured beside these
/// rules: `paredit-feature-lint-safety::handler_case_swallows_error::examine`
/// reads a bounded number of children of its own node, classifies an atom, and
/// never consults the tree. That is what a shipped, local, head-matched
/// `Conditions` rule costs.
#[derive(Debug)]
struct ShippedLocalRule;

const SHIPPED_LOCAL_META: RuleMeta = RuleMeta::new(
    "cost-control-shipped-local",
    RuleCategory::Conditions,
    Severity::Warning,
    "a control with a shipped rule's local child-reading shape",
    Fixability::ReportOnly,
);

static SHIPPED_LOCAL_RULE: ShippedLocalRule = ShippedLocalRule;

/// The broad condition types `handler-case-swallows-error` carries.
const BROAD_TYPES: [&str; 5] = ["error", "condition", "serious-condition", "t", "warning"];

impl LintRule for ShippedLocalRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&ALL_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // The shipped shape: iterate the node's own clauses, read the first
        // child of each, classify it, and look at a bounded body.
        let mut hits = 0_usize;
        for clause in view.children.iter().skip(2) {
            let Some(condition) = clause.children.first().and_then(atom_text) else {
                continue;
            };
            if !symbol_in(condition, &BROAD_TYPES) {
                continue;
            }
            let body = clause.children.get(2..).unwrap_or(&[]);
            let inert = match body {
                [] => true,
                [only] => {
                    only.kind == ExpressionKind::Atom
                        && atom_text(only).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
                }
                _ => false,
            };
            if inert {
                hits += 1;
            }
        }
        std::hint::black_box(hits);
        Ok(())
    }
}

/// The **rejected** rule, preserved exactly as it was written and measured.
///
/// It reads the file's `define-condition` hierarchy to decide whether the type
/// an `error` call names is a warning subtype. The hierarchy cannot be cached
/// across `check()` calls, so this is the per-invocation whole-file cost that
/// kept the rule out of the package. See the module documentation.
#[derive(Debug)]
struct FileHierarchyRule;

const FILE_HIERARCHY_META: RuleMeta = RuleMeta::new(
    "cost-control-file-hierarchy",
    RuleCategory::Conditions,
    Severity::Warning,
    "the rejected rule's shape: a per-invocation whole-file hierarchy read",
    Fixability::ReportOnly,
);

static FILE_HIERARCHY_RULE: FileHierarchyRule = FileHierarchyRule;

/// `error`'s datum is its first argument, `cerror`'s its second.
const HIERARCHY_HEADS: [NormalizedHead; 2] =
    [NormalizedHead::new("error"), NormalizedHead::new("cerror")];

impl LintRule for FileHierarchyRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HIERARCHY_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let hierarchy = crate::support::LazyHierarchy::new(context.tree());
        let datum_index = match crate::support::normalized_symbol(
            paredit_core_syntax::view_query::list_head(view).unwrap_or_default(),
        )
        .as_str()
        {
            "error" => 1,
            "cerror" => 2,
            _ => return Ok(()),
        };
        // The cheap check first — two child lookups — exactly as the rule had it.
        let Some(name) = view
            .children
            .get(datum_index)
            .and_then(crate::support::quoted_symbol)
        else {
            return Ok(());
        };
        let hierarchy = hierarchy.get();
        let reportable = hierarchy.declares(&name) && hierarchy.is_warning_and_not_serious(&name);
        std::hint::black_box(reportable);
        Ok(())
    }
}

// -- the catalogue ------------------------------------------------------------

static ENTRIES: [RuleEntry; 5] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(&SHIPPED_LOCAL_META, &SHIPPED_LOCAL_RULE),
    RuleEntry::new(&FILE_HIERARCHY_META, &FILE_HIERARCHY_RULE),
    RuleEntry::new(
        &crate::condition_type_datum_with_string_initarg::META,
        &crate::condition_type_datum_with_string_initarg::RULE,
    ),
    RuleEntry::new(
        &crate::unwind_protect_cleanup_signals::META,
        &crate::unwind_protect_cleanup_signals::RULE,
    ),
];

const COLUMNS: [&str; 5] = [
    "cost-control-file-hierarchy",
    "condition-type-datum-with-string-initarg",
    "unwind-protect-cleanup-signals",
    "cost-control-shipped-local",
    "cost-control-noop",
];

// -- measurement --------------------------------------------------------------

fn measure(source: &str) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lisp"),
        Dialect::CommonLisp,
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

/// `count` **correct** condition-system forms — the zero-finding shape the
/// `clean/forms/*` benchmarks measure. Every rule must decline all of them, and
/// the cost of declining is what these numbers are.
///
/// Deliberately dense in the things that make these rules work: a
/// `define-condition` (so the hierarchy walk is a real walk and not a byte
/// scan), literally-typed `error` calls on an *error* subtype, and an
/// `unwind-protect` with a real cleanup.
fn clean_source(count: usize) -> String {
    let mut out = String::from(";;; correct condition handling throughout\n");
    out.push_str(
        "(define-condition app-error (error) ((code :initarg :code)) (:report \"app\"))\n",
    );
    for index in 0..count {
        out.push_str(&format!(
            "(defun step{index} (s)\n\
             \x20 (unwind-protect (process{index} s) (close s))\n\
             \x20 (when (bad-p s) (error 'app-error :code {index}))\n\
             \x20 (warn \"slow ~A\" s)\n\
             \x20 (signal 'app-note)\n\
             \x20 (cerror \"Go on.\" \"trouble ~A\" s))\n"
        ));
    }
    out
}

/// The same density with **no** `define-condition` anywhere, which is the shape
/// the hierarchy's byte-scan guard exists for. The difference between this
/// column and the clean one is what the guard buys.
fn no_definition_source(count: usize) -> String {
    let mut out = String::from(";;; literally-typed error calls, no definitions in this file\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defun step{index} (s)\n\
             \x20 (unwind-protect (process{index} s) (close s))\n\
             \x20 (when (bad-p s) (error 'elsewhere-error :code {index})))\n"
        ));
    }
    out
}

/// The reporting shape for `error-signalled-on-warning-condition`: a file that
/// *does* define a warning class and signals it with `error` many times.
///
/// The fixtures that **report** are what expose a per-finding cost. A rule that
/// is cheap while declining and expensive while reporting is exactly the shape
/// `is_unevaluated_at` has, and it is invisible to the clean fixture.
fn warning_heavy_source(count: usize) -> String {
    let mut out = String::from("(define-condition stale (warning) ())\n");
    for index in 0..count {
        out.push_str(&format!("(defun step{index} () (error 'stale))\n"));
    }
    out
}

// -- what runs unattended -----------------------------------------------------

/// The head index must hand every rule exactly the nodes it asked for. Without
/// this, a "cheap" column can simply be a rule that was never called.
#[test]
fn cost_every_rule_is_invoked_once_per_matching_head() {
    let rows = measure(&clean_source(20));
    // Per iteration: 1 unwind-protect, 1 error, 1 warn, 1 signal, 1 cerror.
    //
    // Plus **one** more, which is not an obvious one: the fixture's
    // `(define-condition app-error (error) …)` has `(error)` as its supertype
    // list, and that is a paren list whose head is `error`, so the head index
    // hands it to every rule here as if it were a call. Both rules that read a
    // datum survive it because child 1 does not exist — see
    // `does_not_flag_a_define_condition_supertype_list` in each — but the
    // invocation is real and the count has to say so.
    let noop = invocations_of(&rows, "cost-control-noop");
    assert_eq!(
        noop,
        20 * 5 + 1,
        "five matching heads per generated function, plus the supertype list `(error)`"
    );
    assert_eq!(
        invocations_of(&rows, "cost-control-shipped-local"),
        noop,
        "the two controls declare the same heads and must see the same nodes"
    );

    // Each real rule declares its *own* heads, and the numerator is pinned per
    // rule rather than as equality with the control. A rule that silently
    // covers less than the heads it claims is invisible to an equality check.
    assert_eq!(
        invocations_of(&rows, "condition-type-datum-with-string-initarg"),
        20 * 4 + 1,
        "error, cerror, signal and warn appear once each per function, plus the \
         supertype list; make-condition does not appear in this fixture"
    );
    assert_eq!(
        invocations_of(&rows, "cost-control-file-hierarchy"),
        20 * 2 + 1,
        "error and cerror only, plus the supertype list"
    );
    assert_eq!(
        invocations_of(&rows, "unwind-protect-cleanup-signals"),
        20,
        "unwind-protect only, and a supertype list is not one"
    );
}

/// The rules must decline the clean fixture entirely: a cost number for a rule
/// that is quietly reporting on correct code measures the wrong thing.
#[test]
fn cost_the_clean_fixture_yields_no_findings() {
    let source = clean_source(20);
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lisp"),
        Dialect::CommonLisp,
        &tree,
        &source,
        RuleSelection::All,
        PassOptions::default(),
    )
    .expect("lint pass");
    assert!(
        outcome.outcomes.is_empty(),
        "clean fixture must be clean, got {} findings",
        outcome.outcomes.len()
    );
}

/// The cheap domain check must come before anything that reads the tree.
///
/// The evidence that it does: a fixture whose `error` calls all take a *string*
/// datum costs the hierarchy control almost nothing, even though the file is
/// large and defines a condition — because two child lookups fail first. This is
/// the ordering that siblings measured 450,843 and 8,589,447 ns/call for getting
/// backwards, and it is the reason the rejected rule's cost is as low as it is
/// on files that do not use literal condition types.
#[test]
fn cost_a_string_datum_never_reaches_the_hierarchy() {
    let mut source = String::from("(define-condition app-error (error) () (:report \"app\"))\n");
    for index in 0..400 {
        source.push_str(&format!("(defun step{index} (s) (error \"boom ~A\" s))\n"));
    }
    let rows = measure(&source);
    let hierarchy = nanos_of(&rows, "cost-control-file-hierarchy");
    let noop = nanos_of(&rows, "cost-control-noop");
    // A generous bound: the point is orders of magnitude, not a threshold, and
    // a wall-clock comparison under parallel load cannot carry a tight one.
    assert!(
        hierarchy < noop.saturating_mul(50).max(2_000_000),
        "a string datum must fail the cheap check before the hierarchy walk: \
         hierarchy={hierarchy}ns noop={noop}ns"
    );
}

/// The shipped rules must both stay within an order of magnitude of the shipped
/// *control*, and decisively below the rejected shape, on the zero-finding
/// fixture. This is the regression this file guards.
///
/// Ratios, not absolute durations: under parallel load the durations move
/// together, so a comparison taken in the same pass is the only stable reading.
#[test]
fn cost_the_shipped_rules_are_local() {
    // Deliberately small: this runs in the unoptimized profile, where the
    // rejected shape's quadratic term is expensive enough to dominate the
    // package's whole test run. The separation it asserts is four orders of
    // magnitude, so it does not need a large n to be visible.
    let rows = measure(&clean_source(150));
    let hierarchy = nanos_of(&rows, "cost-control-file-hierarchy");
    let shipped_control = nanos_of(&rows, "cost-control-shipped-local").max(1);

    for rule in [
        "condition-type-datum-with-string-initarg",
        "unwind-protect-cleanup-signals",
    ] {
        let cost = nanos_of(&rows, rule);
        assert!(
            cost < hierarchy,
            "{rule} ({cost}ns) must be cheaper than the rejected shape ({hierarchy}ns)"
        );
        assert!(
            cost / shipped_control < 100,
            "{rule} ({cost}ns) must stay near the shipped control ({shipped_control}ns)"
        );
    }
}

// -- the benchmarks -----------------------------------------------------------

fn report(label: &str, build: impl Fn(usize) -> String) {
    let sizes = [250_usize, 500, 1000, 2000];
    let mut rows = Vec::new();
    for size in sizes {
        rows.push(measure(&build(size)));
    }
    println!("\n== {label} ==");
    for column in COLUMNS {
        let series: Vec<u128> = rows.iter().map(|row| nanos_of(row, column)).collect();
        let first = series.first().copied().unwrap_or(1).max(1);
        let last = series.last().copied().unwrap_or(1);
        let ratio = last / first;
        let invocations = invocations_of(&rows[0], column);
        println!(
            "  {column:44} 8x ratio = {ratio:5}  (n=250 invocations={invocations})  {series:?} ns"
        );
    }
    println!("  (linear is ~8; load average is high, read the ratios not the durations)");
}

#[test]
#[ignore = "benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_clean() {
    report(
        "clean: correct code, file defines a condition",
        clean_source,
    );
}

#[test]
#[ignore = "benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_no_definition() {
    report("no-definition: byte-scan guard path", no_definition_source);
}

#[test]
#[ignore = "benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_warning_heavy() {
    report(
        "warning-heavy: every error call reports",
        warning_heavy_source,
    );
}
