//! A false-positive audit harness: runs every rule in this package over a
//! corpus of third-party Common Lisp and prints findings with their locations.
//!
//! Author-written tests encode the author's model of the language, not the
//! language. Recent batches in this repository killed one rule at 70 findings
//! and 0 true positives over GNU Emacs's sources, another at 15/0 over Guile's,
//! and a third at 33/0 over real Hy. So this exists, and it is `#[ignore]`d only
//! because the corpus is not checked in.
//!
//! ```text
//! PAREDIT_DISPATCH_CORPUS=/path/to/MANIFEST.txt \
//!   cargo test -p paredit-feature-lint-generic-dispatch \
//!   -- --ignored --nocapture ignored_audit_corpus
//! ```
//!
//! The manifest is one absolute path per line. The output reports, per rule,
//! both the finding count **and the denominator** — how many files, how many
//! `defgeneric`/`defmethod`/`defclass` occurrences, and how many occurrences of
//! each rule's own trigger construct. A zero-finding sweep over zero candidates
//! is a false clean and proves nothing at all.
//!
//! [`self_test_finds_the_known_dirty_file`] is *not* ignored. An audit harness
//! that silently produces zero findings because of a broken invocation is the
//! failure mode this repository has already been burned by, so the harness
//! proves on every `cargo test` that it can still see a finding.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::ENTRIES;

/// What each counter in the denominator counts.
const COUNTER_LABELS: [&str; 7] = [
    "(defgeneric",
    "(defmethod",
    "(defclass",
    "(:method",
    "initialize-instance / shared-initialize",
    ":allocation :class",
    "call-next-method",
];

/// The textual occurrences of each construct a rule keys on, so that a finding
/// count has a denominator to be read against.
fn candidate_counts(source: &str) -> [usize; 7] {
    let lower = source.to_ascii_lowercase();
    let allocation_class = lower
        .match_indices(":allocation")
        .filter(|(offset, _)| {
            lower[offset + ":allocation".len()..]
                .trim_start()
                .starts_with(":class")
        })
        .count();
    [
        lower.matches("(defgeneric").count(),
        lower.matches("(defmethod").count(),
        lower.matches("(defclass").count(),
        lower.matches("(:method").count(),
        lower.matches("initialize-instance").count() + lower.matches("shared-initialize").count(),
        allocation_class,
        lower.matches("call-next-method").count(),
    ]
}

/// Every finding this package's rules produce for one file.
fn findings_in(path: &str, source: &str) -> Vec<(String, usize, String)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let Ok(tree) = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp) else {
        return Vec::new();
    };
    let Ok(outcomes) = collect_lint_outcomes(
        catalog,
        &index,
        Path::new(path),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
    ) else {
        return Vec::new();
    };
    outcomes
        .into_iter()
        .map(|outcome| {
            let (finding, _) = outcome.into_parts();
            let offset = finding.span.start().get();
            let line = source[..offset].matches('\n').count() + 1;
            let excerpt = source[offset..]
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .chars()
                .take(110)
                .collect::<String>();
            (finding.rule.to_owned(), line, excerpt)
        })
        .collect()
}

/// One file per rule, each a shape the rule must report.
///
/// The harness's self-test. An invalid invocation that made every batch error
/// out and report zero findings is a false clean that looks exactly like
/// success; this makes that impossible to miss.
const KNOWN_DIRTY: [(&str, &str); 3] = [
    (
        "defgeneric-method-option-incongruent",
        "(defgeneric draw (shape)\n  (:method ((s circle) stream) s))\n",
    ),
    (
        "initialization-primary-without-call-next-method",
        "(defmethod initialize-instance ((o widget) &key) (setup o))\n",
    ),
    (
        "class-allocated-slot-with-initarg",
        "(defclass registry () ((entries :initarg :entries :allocation :class)))\n",
    ),
];

#[test]
fn self_test_finds_the_known_dirty_file() {
    for (rule, source) in KNOWN_DIRTY {
        let found = findings_in("dirty.lisp", source);
        assert_eq!(
            found
                .iter()
                .map(|(name, ..)| name.as_str())
                .collect::<Vec<_>>(),
            vec![rule],
            "the audit harness cannot see {rule}; a corpus sweep through it would be a false clean"
        );
    }
    // ...and the denominator counter really counts, so a zero there means zero.
    let counts = candidate_counts(KNOWN_DIRTY[2].1);
    assert_eq!(counts[2], 1, "(defclass occurrences");
    assert_eq!(counts[5], 1, ":allocation :class occurrences");
}

/// The **internal** denominator: how far into each rule's analysis a corpus
/// actually reaches.
///
/// A textual `(defmethod` count is not this. A rule can be offered ten thousand
/// `defmethod` heads and still decline every one of them at its first guard, in
/// which case its zero findings say nothing about the guards further in. What
/// these count is the last point before the report:
///
/// - for the congruence rule, how many `(:method …)` options were actually
///   read and compared against their own `defgeneric`'s lambda list. Zero
///   comparisons would make its zero findings a false clean, whatever the
///   `defgeneric` count says;
/// - for the initialization rule, how many *primary* methods on one of its
///   generic functions were seen at all;
/// - for the slot rule, how many `:allocation :class` slots were read.
#[derive(Debug, Default)]
struct Reach {
    /// `defgeneric` forms whose lambda list was readable.
    generics_read: usize,
    /// `(:method …)` options compared against their own `defgeneric`.
    options_compared: usize,
    /// `defmethod` forms whose geometry was readable.
    methods_read: usize,
    /// Primary methods on an initialization generic function.
    initialization_primaries: usize,
    /// Slots carrying `:allocation :class`.
    class_slots: usize,
}

impl Reach {
    fn add(&mut self, other: &Self) {
        self.generics_read += other.generics_read;
        self.options_compared += other.options_compared;
        self.methods_read += other.methods_read;
        self.initialization_primaries += other.initialization_primaries;
        self.class_slots += other.class_slots;
    }
}

/// Walks one file the way the rules do and counts how far each one got.
///
/// **Every node, not only the top level.** The head index dispatches a rule on a
/// matched node wherever it sits, and a great deal of real Common Lisp puts its
/// definitions inside a wrapper — every `defclass` in `cffi`'s
/// `toolchain/bundle.lisp` is inside `(with-upgradability () …)`. A first
/// version of this counter walked `root.children` alone and reported *one*
/// `:allocation :class` slot where the rules had actually been offered
/// twenty-seven, which would have made a real denominator look like an untested
/// one.
fn reach_in(source: &str) -> Reach {
    let mut reach = Reach::default();
    let Ok(tree) = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp) else {
        return reach;
    };
    for child in &tree.root_view().children {
        count_node(child, &mut reach);
    }
    reach
}

/// Counts one node and everything under it.
///
/// Written as an explicit recursion rather than through `for_each_subview`,
/// whose callback is a higher-ranked closure and cannot lend a `&ExpressionView`
/// to anything outside itself.
fn count_node(form: &paredit_core_syntax::sexpr::ExpressionView, reach: &mut Reach) {
    use crate::support::{is_method_option, lambda_list_of, method_parts, symbol_name};
    use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

    for child in &form.children {
        count_node(child, reach);
    }
    let Some(head) = list_head(form) else {
        return;
    };
    if symbol_is(head, "defgeneric") {
        let readable = form
            .children
            .get(2)
            .filter(|child| is_paren_list(child))
            .and_then(lambda_list_of);
        if readable.is_none() {
            return;
        }
        reach.generics_read += 1;
        reach.options_compared += form
            .children
            .iter()
            .skip(3)
            .filter(|child| is_method_option(child))
            .filter(|child| {
                crate::support::method_option_parts(child)
                    .and_then(|parts| lambda_list_of(parts.lambda_list))
                    .is_some()
            })
            .count();
    } else if symbol_is(head, "defmethod") {
        let Some(parts) = method_parts(form) else {
            return;
        };
        reach.methods_read += 1;
        if parts.is_primary()
                && parts.generic_name().is_some_and(|name| {
                    crate::initialization_primary_without_call_next_method::domain::is_initialization_generic(&name)
                })
            {
                reach.initialization_primaries += 1;
            }
    } else if symbol_is(head, "defclass") {
        let Some(slots) = form.children.get(3).filter(|child| is_paren_list(child)) else {
            return;
        };
        for slot in &slots.children {
            let mut index = 1;
            while index < slot.children.len() {
                if symbol_name(&slot.children[index]).as_deref() == Some(":allocation")
                    && slot
                        .children
                        .get(index + 1)
                        .and_then(symbol_name)
                        .as_deref()
                        == Some(":class")
                {
                    reach.class_slots += 1;
                }
                index += 2;
            }
        }
    }
}

#[test]
#[ignore = "needs a third-party corpus; set PAREDIT_DISPATCH_CORPUS to a manifest"]
fn ignored_audit_corpus() {
    let Ok(manifest_path) = std::env::var("PAREDIT_DISPATCH_CORPUS") else {
        panic!("set PAREDIT_DISPATCH_CORPUS to a manifest of absolute .lisp paths");
    };
    let manifest = std::fs::read_to_string(&manifest_path).expect("read the manifest");

    let mut files_scanned = 0usize;
    let mut files_parsed = 0usize;
    let mut totals = [0usize; 7];
    let mut reach = Reach::default();
    let mut findings: Vec<(String, String, usize, String)> = Vec::new();

    for line in manifest.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        files_scanned += 1;
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        for (slot, count) in candidate_counts(&source).into_iter().enumerate() {
            totals[slot] += count;
        }
        if SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).is_err() {
            continue;
        }
        files_parsed += 1;
        reach.add(&reach_in(&source));
        for (rule, line_number, excerpt) in findings_in(path, &source) {
            findings.push((rule, path.to_owned(), line_number, excerpt));
        }
    }

    println!("\n===== DENOMINATOR =====");
    println!("files in manifest      : {files_scanned}");
    println!("files parsed as CL     : {files_parsed}");
    for (label, total) in COUNTER_LABELS.iter().zip(totals) {
        println!("{label:<40}: {total}");
    }

    println!("\n===== HOW FAR EACH RULE ACTUALLY GOT =====");
    println!("defgeneric forms read       : {}", reach.generics_read);
    println!("(:method ...) options COMPARED: {}", reach.options_compared);
    println!("defmethod forms read        : {}", reach.methods_read);
    println!(
        "PRIMARY initialization methods: {}",
        reach.initialization_primaries
    );
    println!(":allocation :class slots read : {}", reach.class_slots);

    println!("\n===== FINDINGS BY RULE =====");
    for entry in RuleCatalog::new(&ENTRIES).entries() {
        let name = entry.meta().name().as_str();
        let count = findings.iter().filter(|(rule, ..)| rule == name).count();
        println!("{count:>6}  {name}");
    }

    println!("\n===== EVERY FINDING =====");
    for (rule, path, line, excerpt) in &findings {
        println!("{rule}\t{path}:{line}\t{excerpt}");
    }
    println!("\ntotal findings: {}", findings.len());
}
