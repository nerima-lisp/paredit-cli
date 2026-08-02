//! The false-positive gate: all three added rules, run together through the
//! real engine, over code that is *correct*.
//!
//! Unit tests written alongside a rule encode the author's model of the rule,
//! which is the same model that produced the rule. They cannot catch a rule that
//! fires on an idiom the author never thought to write down — and a performance
//! rule that fires on ordinary code gets the whole category disabled, which is
//! worse than the rule not existing.
//!
//! So this module runs the three rules over two corpora that were not written to
//! exercise them:
//!
//! - [`IDIOMATIC`], hand-written Common Lisp that deliberately walks past every
//!   trigger — chained maps that cannot fuse, sorts whose ordering is kept,
//!   hash-table code that writes as well as reads — and must produce nothing.
//! - Every `.lisp` file tracked in this repository, which exists for parser,
//!   formatter, and golden-lint reasons and knows nothing about these rules.
//!
//! "No findings" only means something if the harness can produce findings at
//! all, so [`DANGEROUS_TWIN`] is the same file with each idiom bent into the
//! shape the rules are for, and is asserted to produce exactly one finding per
//! rule. Without that pair, a broken harness reports a clean sweep.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::redundant_full_sequence_traversal_fusable_maps as fusable_maps;
use crate::repeated_hash_table_lookup_same_key as repeated_lookup;
use crate::unnecessary_sort_before_extremum_extraction as sorted_extremum;

/// The three rules this batch added, run as one catalogue.
///
/// The shipped registry lives in the root crate and this package must not name
/// it, so the sweep builds its own — which is also what makes the sweep report
/// *these* rules' findings and nothing else's.
static ADDED_RULES: [RuleEntry; 3] = [
    RuleEntry::new(&fusable_maps::META, &fusable_maps::RULE),
    RuleEntry::new(&sorted_extremum::META, &sorted_extremum::RULE),
    RuleEntry::new(&repeated_lookup::META, &repeated_lookup::RULE),
];

/// Every finding the three rules make over one source, as
/// `(rule name, the exact text of the reported form)`.
fn sweep(path: &Path, source: &str) -> Vec<(&'static str, String)> {
    let catalog = RuleCatalog::new(&ADDED_RULES);
    let index = build_head_index(catalog);
    let Ok(tree) = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp) else {
        // A fixture that exists to be unparseable is not this sweep's subject.
        return Vec::new();
    };
    collect_lint_outcomes(
        catalog,
        &index,
        path,
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint")
    .into_iter()
    .map(|outcome| {
        let finding = outcome.into_parts().0;
        let span = finding.span;
        (
            finding.rule,
            source[span.start().get()..span.end().get()].to_owned(),
        )
    })
    .collect()
}

/// Correct, ordinary Common Lisp that sits next to every trigger without being
/// one.
///
/// Each definition is annotated with which rule it is aimed at and why it must
/// stay silent. A finding here is a false positive, full stop.
const IDIOMATIC: &str = r#"
;;; ---- fusable maps: chains that must not be fused -----------------------

;; One map. Nothing to fuse.
(defun entry-names (entries)
  (mapcar #'entry-name entries))

;; A map over a *filter*, not over another map: there is no intermediate
;; sequence that exists only to be walked, because `remove-if` is already the
;; thing narrowing the input.
(defun active-names (entries)
  (mapcar #'entry-name (remove-if #'entry-retired-p entries)))

;; A zip. `(mapcar #'+ as bs)` produces one element per pair, and no single
;; composed function replaces it.
(defun weighted-total (weights values)
  (reduce #'+ (mapcar #'* weights values)))

;; A map over a splice. `mapcan` does not produce one element per input element.
(defun all-children (nodes)
  (mapcar #'node-id (mapcan #'node-children nodes)))

;; A map over a map with a *different* operator: reading this as a fusable pair
;; needs the inner result type resolved, which nothing here does.
(defun coerce-and-round (xs)
  (mapcar #'round (map 'list #'float xs)))

;; Two maps over the same sequence, held separately. Fusing them would need the
;; first result to be provably dead, which it is not.
(defun split-report (entries)
  (let ((names (mapcar #'entry-name entries))
        (sizes (mapcar #'entry-size entries)))
    (values names sizes (length names))))

;;; ---- sort: orderings that are kept, and top-k -------------------------

;; The ordering is the point.
(defun ranked (entries)
  (sort (copy-list entries) #'> :key #'entry-score))

;; The ordering is bound and read at both ends, so it is not discarded.
(defun score-range (entries)
  (let ((ranked (sort (copy-list entries) #'< :key #'entry-score)))
    (list (first ranked) (car (last ranked)))))

;; Top-k. Sorting is a reasonable way to get three elements.
(defun top-three (entries)
  (subseq (sort (copy-list entries) #'> :key #'entry-score) 0 3))

;; The last *three* conses, not the last one.
(defun tail-three (entries)
  (last (sort (copy-list entries) #'<) 3))

;; An order statistic that is not an extremum.
(defun runner-up (entries)
  (second (sort (copy-list entries) #'> :key #'entry-score)))

;; The extremum, already written the way this rule would suggest. A rule that
;; fired on its own recommendation would be the worst kind of false positive.
(defun cheapest (entries)
  (reduce #'min entries :key #'entry-cost))

(defun latest (entries)
  (loop for entry in entries maximize (entry-timestamp entry)))

;; `first` of something that is not a sort.
(defun head-of-queue (queue)
  (first (remove-if #'entry-retired-p queue)))

;;; ---- hash tables: reads that are not repeats --------------------------

;; The idiomatic single lookup with its presence flag.
(defun lookup (table key)
  (multiple-value-bind (value present) (gethash key table)
    (if present value :missing)))

;; Two lookups, two keys.
(defun bounds (limits)
  (cons (gethash :minimum limits) (gethash :maximum limits)))

;; Two lookups of one key in arms that cannot both run.
(defun policy-for (table urgent)
  (if urgent
      (gethash :fast-path table)
      (gethash :fast-path table)))

;; The table is written between the reads, so the second read can differ.
(defun ensure-entry (table)
  (or (gethash :entry table)
      (setf (gethash :entry table) (make-entry))))

;; The table is handed to a callee, which may write it.
(defun refreshed-limit (table)
  (let ((before (gethash :limit table)))
    (refresh-policy table)
    (list before (gethash :limit table))))

;; A computed key: `key` may hold something else at the second call.
(defun paired (table key)
  (list (gethash key table) (gethash key table)))

;; A lookup already bound once, which is what the rule recommends.
(defun formatted-limit (table)
  (let ((limit (gethash :limit table)))
    (if limit (format nil "~D" limit) "none")))

;; Reads of two different tables.
(defun merged (primary fallback)
  (or (gethash :timeout primary) (gethash :timeout fallback)))

;; A lookup inside a macro template is data, not a call.
(defmacro with-limit ((var table) &body body)
  `(let ((,var (gethash :limit ,table)))
     ,@body))
"#;

/// [`IDIOMATIC`], bent into the shapes the rules are for.
///
/// Exactly one finding per rule, so a harness that has stopped reporting
/// anything fails here rather than passing the sweep silently.
const DANGEROUS_TWIN: &str = r#"
(defun entry-name-initials (entries)
  (mapcar #'first (mapcar #'entry-name entries)))

(defun cheapest-entry (entries)
  (first (sort (copy-list entries) #'< :key #'entry-cost)))

(defun described-limit (table)
  (if (gethash :limit table)
      (format nil "limit ~D" (gethash :limit table))
      "no limit"))
"#;

/// Every `.lisp` file under `root`, recursively.
fn lisp_files(root: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            lisp_files(&path, into);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "lisp")
        {
            into.push(path);
        }
    }
}

// ---------------------------------------------------------------------------
// Cost
// ---------------------------------------------------------------------------

/// The three added rules plus one already-shipped rule as a control.
///
/// A number in microseconds means nothing on its own — a loaded machine moves it
/// by an order of magnitude. `loop-invariant-allocation` is measured in the same
/// pass, on the same file, so the comparison survives whatever the machine is
/// doing.
static MEASURED_RULES: [RuleEntry; 4] = [
    RuleEntry::new(&fusable_maps::META, &fusable_maps::RULE),
    RuleEntry::new(&sorted_extremum::META, &sorted_extremum::RULE),
    RuleEntry::new(&repeated_lookup::META, &repeated_lookup::RULE),
    RuleEntry::new(
        &crate::loop_invariant_allocation::META,
        &crate::loop_invariant_allocation::RULE,
    ),
];

/// `count` definitions of *clean* Common Lisp, dense in every head the four
/// measured rules anchor on.
///
/// Clean on purpose: the `clean/forms/*` benchmarks that gate this repository
/// lint files with zero findings, so what they measure is exactly the per-file
/// cost of a rule that matches nothing. A generator that produced findings would
/// measure the wrong thing.
///
/// `with_table` decides whether the definitions spell `gethash`, which is what
/// `repeated-hash-table-lookup-same-key`'s byte-scan gate keys on — so the two
/// settings measure the walk and the gate separately.
fn generated(count: usize, with_table: bool) -> String {
    let table = if with_table {
        "(push (cons x (gethash (entry-key x) h)) *log*)"
    } else {
        "(push (cons x (entry-key x)) *log*)"
    };
    (0..count)
        .map(|index| {
            format!(
                "(defun handler-{index} (h xs ys)\n  \
                   (declare (ignorable h))\n  \
                   (let ((names (mapcar #'entry-name xs))\n        \
                         (sizes (map 'list #'entry-size ys)))\n    \
                     (dolist (x xs) {table})\n    \
                     (loop for y in ys do (note y))\n    \
                     (dotimes (i 10) (note i))\n    \
                     (sort (copy-list names) #'string<)\n    \
                     (list (first names) (last sizes) sizes)))\n"
            )
        })
        .collect()
}

/// Per-rule wall time and invocation count over one source, through the same
/// machinery `inspect lint --timings` uses.
fn measure(source: &str) -> Vec<(&'static str, std::time::Duration, u64)> {
    use paredit_core_lint_engine::engine::{PassOptions, collect_lint_pass};

    let catalog = RuleCatalog::new(&MEASURED_RULES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("bench.lisp"),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("lint");
    assert!(
        outcome.outcomes.is_empty(),
        "the cost fixture must be clean, or it is measuring the wrong thing: {:?}",
        outcome
            .outcomes
            .into_iter()
            .map(|found| found.into_parts().0.message)
            .collect::<Vec<_>>()
    );
    let timings = outcome.timings.expect("measure was requested");
    timings
        .entries()
        .map(|(position, elapsed, invocations)| {
            (
                MEASURED_RULES[position].meta().name().as_str(),
                elapsed,
                invocations,
            )
        })
        .collect()
}

/// The repository root, reached from this package's manifest directory.
///
/// `packages/feature/lint-performance` is three levels down. Nothing is written
/// and a missing directory is skipped, so this cannot fail a sandboxed build
/// that has fewer files than a checkout.
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idiomatic_code_produces_no_findings() {
        let found = sweep(Path::new("idiomatic.lisp"), IDIOMATIC);
        assert!(
            found.is_empty(),
            "these are false positives on correct code: {found:#?}"
        );
    }

    /// The control for the test above. Without it, "no findings" is also what a
    /// harness that runs no rules reports.
    #[test]
    fn the_dangerous_twin_produces_exactly_one_finding_per_rule() {
        let found = sweep(Path::new("twin.lisp"), DANGEROUS_TWIN);
        let mut rules: Vec<&str> = found.iter().map(|(rule, _)| *rule).collect();
        rules.sort_unstable();
        assert_eq!(
            rules,
            vec![
                "redundant-full-sequence-traversal-fusable-maps",
                "repeated-hash-table-lookup-same-key",
                "unnecessary-sort-before-extremum-extraction",
            ],
            "the harness must still be able to find things: {found:#?}"
        );
    }

    /// The corpus that was not written for these rules.
    ///
    /// Every `.lisp` file in the repository — parser fixtures, formatter
    /// idempotence corpora, golden-lint inputs, migration recipes — none of
    /// which knows these rules exist. A finding here is a finding on code
    /// somebody wrote for another purpose entirely, which is the closest thing
    /// to "realistic" this repository contains.
    #[test]
    fn the_repositorys_own_lisp_fixtures_produce_no_findings() {
        let mut files = Vec::new();
        lisp_files(&repository_root(), &mut files);
        assert!(
            files.len() >= 10,
            "expected the repository's .lisp fixtures; found {}",
            files.len()
        );
        let mut found = Vec::new();
        for file in &files {
            let Ok(source) = std::fs::read_to_string(file) else {
                continue;
            };
            for finding in sweep(file, &source) {
                found.push((file.display().to_string(), finding));
            }
        }
        assert!(
            found.is_empty(),
            "these are false positives on the repository's own fixtures: {found:#?}"
        );
    }

    /// Every node in `source`, so an invocation count can be read against the
    /// number of nodes a per-node dispatch would have produced.
    fn node_count(source: &str) -> u64 {
        fn walk(view: &paredit_core_syntax::sexpr::ExpressionView) -> u64 {
            1 + view.children.iter().map(walk).sum::<u64>()
        }
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        tree.root_view().children.iter().map(walk).sum()
    }

    /// **Invocation counts.** Each rule must be called once per node its head
    /// filter matches, and no more. A rule that started walking the tree itself
    /// would show one invocation and a large elapsed time instead.
    ///
    /// This is the half of the cost property that is deterministic: the head
    /// index decides it before any `check` body runs, so the numbers are the
    /// same on a loaded machine and an idle one. The growth-rate half is in
    /// [`ignored_bench_doubling_ratio`], which does not run unattended.
    ///
    /// The node count is the control: without it, "800 invocations" is also
    /// what a per-node dispatch would report on a file of 800 nodes.
    #[test]
    fn each_added_rule_is_dispatched_once_per_matching_head() {
        let source = generated(400, true);
        let nodes = node_count(&source);
        assert!(
            nodes > 400 * 10,
            "the fixture has {nodes} nodes for 400 definitions; a per-head count cannot be \
             told apart from a per-node one"
        );

        let rows = measure(&source);
        let calls = |rule: &str| {
            rows.iter()
                .find(|(name, _, _)| *name == rule)
                .expect("measured")
                .2
        };
        // 400 definitions, each with one `mapcar`, one `map`, one `first`, one
        // `last`, and one `defun`.
        assert_eq!(
            calls("redundant-full-sequence-traversal-fusable-maps"),
            800,
            "one call per mapcar/map, not per node ({nodes} nodes)"
        );
        assert_eq!(
            calls("unnecessary-sort-before-extremum-extraction"),
            800,
            "one call per first/last, not per node ({nodes} nodes)"
        );
        assert_eq!(
            calls("repeated-hash-table-lookup-same-key"),
            400,
            "one call per defun, not per node ({nodes} nodes)"
        );
    }

    /// The growth rate, and the byte-scan gate — both wall-clock, so both are
    /// benchmarks rather than tests.
    ///
    /// **Linearity.** Doubling the definition count should roughly double the
    /// cost. A rule that rescans the file per match shows ~3.7 per doubling,
    /// which is the shape two shipped rules had, and which this table found in
    /// four rules in this batch.
    ///
    /// **The byte-scan gate.** A definition that never spells `gethash` must not
    /// pay for the body walk; `loop-invariant-allocation` in the same pass is
    /// the control. This one previously asserted `lookup < control * 4` and was
    /// defended as machine-independent because it compares two measurements
    /// rather than a clock. That defence does not hold: the ratio of two short
    /// durations is itself unstable under load, and worst where the numerator
    /// is smallest — which is the case being asserted. There is no deterministic
    /// equivalent, because the gate short-circuits *inside* `check`, where the
    /// invocation count cannot see it. So it prints.
    ///
    /// ```text
    /// cargo test -p paredit-feature-lint-performance \
    ///   -- --ignored --nocapture ignored_bench_doubling_ratio
    /// ```
    #[test]
    #[ignore = "a benchmark: wall-clock ratios are unstable under parallel load"]
    fn ignored_bench_doubling_ratio() {
        // Warm the allocator and the caches so the first measurement is not the
        // one that pays for them.
        measure(&generated(64, true));

        let small = measure(&generated(400, true));
        let large = measure(&generated(800, true));

        println!("\n  rule                                                  µs  invocations");
        for ((name, elapsed, invocations), (_, wide, wide_calls)) in small.iter().zip(&large) {
            let ratio = wide.as_secs_f64() / elapsed.as_secs_f64().max(f64::EPSILON);
            println!(
                "  {name:<50} {:>7.0} {invocations:>12}   \
                 (at 2x: {:>7.0}µs, {wide_calls} calls, ratio {ratio:.2})",
                elapsed.as_micros(),
                wide.as_micros(),
            );
        }
        println!("  linear is ~2.00; a per-match whole-file scan is ~3.70");

        measure(&generated(64, false));
        let gated = measure(&generated(600, false));
        let elapsed = |rule: &str| {
            gated
                .iter()
                .find(|(name, _, _)| *name == rule)
                .expect("measured")
                .1
        };
        println!(
            "\n  gated lookup rule: {}µs over 600 definitions with no `gethash`; \
             control loop-invariant-allocation: {}µs (comparable means the gate short-circuits)",
            elapsed("repeated-hash-table-lookup-same-key").as_micros(),
            elapsed("loop-invariant-allocation").as_micros()
        );
    }

    /// The sweep must actually be reading files, or the test above proves
    /// nothing about them.
    #[test]
    fn the_fixture_sweep_reads_real_files() {
        let mut files = Vec::new();
        lisp_files(&repository_root(), &mut files);
        let total: usize = files
            .iter()
            .filter_map(|file| std::fs::read_to_string(file).ok())
            .map(|source| source.len())
            .sum();
        assert!(
            total > 1000,
            "the sweep read {total} bytes; it is not reading the fixtures"
        );
    }
}
