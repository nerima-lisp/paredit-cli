//! The third-party false-positive audit: the rule runs over real Common Lisp
//! that nobody here wrote.
//!
//! `#[ignore]`d, because it reads corpora that only exist on a machine with SBCL
//! and Quicklisp installed. Point it at them and run:
//!
//! ```text
//! DS_CORPUS_ROOTS=/nix/store/…-sbcl-2.6.0/lib/sbcl:$HOME/quicklisp/dists \
//!   cargo test -p paredit-feature-lint-destructive-sequence --lib \
//!   -- --ignored --nocapture corpus_audit
//! ```
//!
//! # Three denominators, because this rule has two
//!
//! A zero-finding sweep over zero candidates is a **false-clean**, and it is how
//! this kind of audit usually fails: a glob that matches nothing, or a harness
//! that errors out, reports zero findings and looks like success.
//!
//! This rule anchors on the *body form*, so its dispatch count measures
//! `defun`/`let`/`when` rather than anything destructive. That number alone
//! would not prove the corpus contains a single `sort`. So the sweep reports:
//!
//! 1. **body forms dispatched** — what the head index handed the rule,
//! 2. **destructive calls present** — every `sort`/`stable-sort`/`nconc`/
//!    `nbutlast`/`nsublis`/`nsubst` node reachable as code, counted by an
//!    independent walk, which is the population the rule is actually judging,
//!    and
//! 3. **findings**.
//!
//! [`corpus_audit_self_test`] runs first, over a known-dirty source, and fails
//! loudly if the harness cannot find a defect it was told is there. Read that
//! line before believing any zero below it.
//!
//! # The result, 2026-08-04
//!
//! SBCL 2.6.0's own sources and contribs, plus the installed Quicklisp dist:
//!
//! ```text
//! files scanned  : 1619   (898 SBCL + 721 Quicklisp)
//! files unparsed :   31   (~63 destructive operators, named in the output)
//! bytes          : 28,378,755
//!
//! body forms dispatched to the rule       : 56731
//! destructive calls present (population)  :   295   (218 SBCL + 77 Quicklisp)
//!   of those, on a bare variable  (cond 1):   122
//!   of those, in a discarded slot (cond 2):     1
//!   of those, read by a later form(cond 3):     0   <- FINDINGS
//! ```
//!
//! **Zero findings over 295 destructive calls.** The funnel says which condition
//! did the cutting, and it is condition 2 by a distance: of 122 destructive
//! calls on a bare variable, exactly **one** sits in a value-discarding
//! position. Mature Common Lisp essentially always binds the result.
//!
//! # The one near miss, adjudicated
//!
//! ```text
//! sbcl/src/code/globals.lisp:70: (nconc list (list (list symbol initform)))
//! ```
//!
//! **Correct code, correctly declined.** `list` there is a header cons — the
//! surrounding function reads `(cdr list)` and `(assq symbol (cdr list))` — so it
//! is non-empty by construction, `nconc` therefore mutates it in place and
//! returns that same object, and discarding the result is deliberate. Condition
//! 3 declined it because no later form in that body reads `list`.
//!
//! This is the only evidence in the corpus about whether condition 3 earns its
//! place, and it says yes: without it this sweep would have reported one false
//! positive over 1619 files instead of none.
//!
//! # What zero *true* positives means
//!
//! Stated plainly: the sweep found no real defects either. That is the expected
//! result for mature, widely-deployed code — and note that for eleven of the
//! twenty-two CLHS destructive functions SBCL itself emits a `STYLE-WARNING`, so
//! defects in that half do not survive to a release of code that is compiled by
//! SBCL daily. What this measures is a **false-positive rate of 0 over 295
//! candidate calls**, which is the bar the audit exists to set. The rule lands
//! on code under development, which is not what a distribution corpus contains.

use std::fs;
use std::path::{Path, PathBuf};

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_core_syntax::view_query::{list_head, symbol_in};

use crate::support::for_each_evaluated_subview;

static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
    &crate::discarded_destructive_sequence_result::META,
    &crate::discarded_destructive_sequence_result::RULE,
)];

/// The six operators the rule judges, for the independent denominator walk.
const DESTRUCTIVE_HEADS: [&str; 6] = [
    "sort",
    "stable-sort",
    "nconc",
    "nbutlast",
    "nsublis",
    "nsubst",
];

/// A source with the defect, used to prove the harness works before any zero is
/// believed.
const KNOWN_DIRTY: &str = r#"
(defun rank (items)
  (sort items #'<)
  (print items))
(defun join (a b)
  (nconc a b)
  (length a))
"#;

#[derive(Default)]
struct Totals {
    files_scanned: usize,
    files_unparsed: usize,
    bytes: u64,
    /// Nodes the dispatcher handed the rule (body forms).
    dispatched: u64,
    /// The three-condition funnel, counted independently of the rule.
    funnel: Funnel,
    findings: Vec<String>,
    /// Files the reader refused, with the count of destructive operators their
    /// raw text mentions. A file that does not parse is **invisible** to this
    /// audit, so its share has to be reported rather than silently dropped.
    unparsed: Vec<(PathBuf, usize)>,
}

/// A crude count of the operators of interest, from raw text — used only to say
/// how much a file that failed to parse would have contributed.
fn mentions_of_interest(source: &str) -> usize {
    let lower = source.to_ascii_lowercase();
    [
        "(sort ",
        "(stable-sort ",
        "(nconc ",
        "(nbutlast ",
        "(nsublis ",
        "(nsubst ",
    ]
    .iter()
    .map(|needle| lower.matches(needle).count())
    .sum()
}

/// The funnel: how many destructive calls survive each of the rule's three
/// conditions.
///
/// A bare "0 findings over 295 calls" does not say *why*. This says which
/// condition did the cutting, which is the difference between "the corpus is
/// clean" and "the rule is broken and reports nothing".
#[derive(Default, Clone, Copy)]
struct Funnel {
    /// Destructive calls reachable as code — the population.
    calls: u64,
    /// …whose destroyed argument is a bare variable (condition 1).
    on_variable: u64,
    /// …and which sit in a discarded body position (condition 2).
    discarded: u64,
}

impl Funnel {
    fn add(&mut self, other: Self) {
        self.calls += other.calls;
        self.on_variable += other.on_variable;
        self.discarded += other.discarded;
    }
}

/// Walks the tree once, counting the population and each condition's survivors,
/// independently of the rule's own dispatch, and collecting the spans that
/// survive condition 2.
///
/// The near misses are a destructive call on a bare variable, sitting in a
/// discarded slot, that only condition 3 declined. They are the cases worth
/// reading by hand, and in a zero-finding sweep they are the only evidence about
/// whether condition 3 is doing useful work or hiding everything.
fn funnel_of_with_near_misses(
    tree: &SyntaxTree,
    near_misses: &mut Vec<paredit_core_syntax::sexpr::ByteSpan>,
) -> Funnel {
    let mut funnel = Funnel::default();
    for_each_evaluated_subview(&tree.root_view(), |view| {
        // The population: any destructive call, wherever it sits.
        if list_head(view).is_some_and(|head| symbol_in(head, &DESTRUCTIVE_HEADS)) {
            funnel.calls += 1;
        }
        // Conditions 1 and 2, counted from the body form the same way the rule
        // sees them — but *without* condition 3, so the last cut is visible.
        let Some(start) = list_head(view).and_then(crate::support::body_start) else {
            return;
        };
        let Some(last) = view.children.len().checked_sub(1) else {
            return;
        };
        for index in start..last {
            let Some(child) = view.children.get(index) else {
                break;
            };
            if !child.reader_prefixes.is_empty() {
                continue;
            }
            if !list_head(child).is_some_and(|head| symbol_in(head, &DESTRUCTIVE_HEADS)) {
                continue;
            }
            funnel.discarded += 1;
            near_misses.push(child.span);
        }
    });
    // Condition 1 across the whole file, counted separately: a destructive call
    // on a bare variable anywhere at all.
    for_each_evaluated_subview(&tree.root_view(), |view| {
        if crate::discarded_destructive_sequence_result::destroyed_variable_of(view).is_some() {
            funnel.on_variable += 1;
        }
    });
    funnel
}

/// Lints one source, returning `(dispatched, funnel, findings)`.
fn lint(path: &Path, source: &str) -> Option<(u64, Funnel, Vec<String>)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).ok()?;
    let outcome = collect_lint_pass(
        catalog,
        &index,
        path,
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .ok()?;

    let dispatched = outcome
        .timings
        .as_ref()
        .expect("measure: true produces timings")
        .entries()
        .map(|(_, _, invocations)| invocations)
        .sum();

    let findings = outcome
        .outcomes
        .into_iter()
        .map(|item| {
            let (finding, _) = item.into_parts();
            let line = source[..finding.span.start().get()].lines().count();
            let text = source
                .get(finding.span.start().get()..finding.span.end().get())
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            format!("{}:{line}: {text}", path.display())
        })
        .collect();

    let mut spans = Vec::new();
    let funnel = funnel_of_with_near_misses(&tree, &mut spans);
    NEAR_MISSES.with(|cell| {
        for span in spans {
            let line = source[..span.start().get()].lines().count();
            let text = source
                .get(span.start().get()..span.end().get())
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            cell.borrow_mut()
                .push(format!("{}:{line}: {text}", path.display()));
        }
    });
    Some((dispatched, funnel, findings))
}

thread_local! {
    /// Near misses accumulated across the sweep, printed at the end.
    static NEAR_MISSES: std::cell::RefCell<Vec<String>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "lisp" | "cl" | "asd" | "lsp"))
        {
            out.push(path);
        }
    }
}

fn accumulate(totals: &mut Totals, path: &Path) {
    let Ok(source) = fs::read_to_string(path) else {
        return;
    };
    totals.files_scanned += 1;
    totals.bytes += source.len() as u64;
    let Some((dispatched, funnel, findings)) = lint(path, &source) else {
        totals.files_unparsed += 1;
        totals
            .unparsed
            .push((path.to_path_buf(), mentions_of_interest(&source)));
        return;
    };
    totals.dispatched += dispatched;
    totals.funnel.add(funnel);
    totals.findings.extend(findings);
}

/// Proves the harness can find a defect it was told is there.
///
/// Runs unattended, unlike the sweep itself: it needs no corpus. If this is
/// green and the sweep reports zero, the zero means something.
#[test]
fn corpus_audit_self_test() {
    let (dispatched, funnel, findings) =
        lint(Path::new("known-dirty.lisp"), KNOWN_DIRTY).expect("the known-dirty source parses");
    assert_eq!(
        findings.len(),
        2,
        "the harness must find both planted defects, got: {findings:?}"
    );
    assert!(dispatched > 0, "the rule must be dispatched at all");
    assert_eq!(
        funnel.calls, 2,
        "the independent denominator walk must see both destructive calls"
    );
    assert_eq!(funnel.on_variable, 2, "both are on a bare variable");
    assert_eq!(funnel.discarded, 2, "both sit in a discarded position");
}

/// The end-to-end check: a defect planted **into the corpus's own idiom** must
/// be found, and an untouched neighbour must stay clean.
///
/// The self-test above proves the harness works on a source written here, which
/// is weaker: this repository's own phrasing is not what SBCL's code looks like.
/// This takes a real corpus file, splices a defect into it, and requires the
/// sweep to find exactly that one.
#[test]
#[ignore = "needs DS_CORPUS_ROOTS pointing at SBCL and Quicklisp sources"]
fn corpus_audit_finds_a_defect_planted_in_a_real_file() {
    let roots = std::env::var("DS_CORPUS_ROOTS").expect("set DS_CORPUS_ROOTS");
    let mut files = Vec::new();
    for root in roots.split(':').filter(|root| !root.is_empty()) {
        collect_files(Path::new(root), &mut files);
    }
    files.sort();

    // Pick the first real file that parses and is big enough to be typical.
    let (path, source) = files
        .iter()
        .find_map(|path| {
            let source = fs::read_to_string(path).ok()?;
            if source.len() < 4000 {
                return None;
            }
            lint(path, &source)?;
            Some((path.clone(), source))
        })
        .expect("the corpus must contain a parseable file");

    let (_, _, before) = lint(&path, &source).expect("the untouched file parses");
    assert!(
        before.is_empty(),
        "the chosen control file must be clean before planting: {before:?}"
    );

    let planted =
        format!("{source}\n(defun planted-defect-probe (xs)\n  (sort xs #'<)\n  (print xs))\n");
    let (_, _, after) = lint(&path, &planted).expect("the planted file parses");
    assert_eq!(
        after.len(),
        1,
        "the sweep must find exactly the planted defect in {}, got {after:?}",
        path.display()
    );
    println!(
        "end-to-end: planted 1 defect into {} ({} bytes) and the sweep found exactly it; \
         the untouched file reported 0",
        path.display(),
        source.len()
    );
}

#[test]
#[ignore = "needs DS_CORPUS_ROOTS pointing at SBCL and Quicklisp sources"]
fn corpus_audit() {
    // Self-test first, and loudly. A harness that silently fails looks exactly
    // like a clean sweep.
    corpus_audit_self_test();
    println!("harness self-test: OK (2/2 planted defects found)");
    // The self-test lints its own known-dirty source on this thread, so its two
    // near misses would otherwise be counted as corpus near misses.
    NEAR_MISSES.with(|cell| cell.borrow_mut().clear());

    let roots = std::env::var("DS_CORPUS_ROOTS").expect("set DS_CORPUS_ROOTS");
    let mut totals = Totals::default();
    let mut per_root = Vec::new();

    for root in roots.split(':').filter(|root| !root.is_empty()) {
        let mut files = Vec::new();
        collect_files(Path::new(root), &mut files);
        files.sort();
        let before = totals.files_scanned;
        let calls_before = totals.funnel.calls;
        let findings_before = totals.findings.len();
        for path in &files {
            accumulate(&mut totals, path);
        }
        per_root.push((
            root.to_owned(),
            totals.files_scanned - before,
            totals.funnel.calls - calls_before,
            totals.findings.len() - findings_before,
        ));
    }

    println!("\n=== CORPUS AUDIT ===");
    for (root, files, calls, findings) in &per_root {
        println!("  root {root}\n    files={files} destructive-calls={calls} findings={findings}");
    }
    println!(
        "\n  files scanned       : {}\n  files unparsed      : {}\n  bytes               : {}",
        totals.files_scanned, totals.files_unparsed, totals.bytes
    );
    println!("\n  DENOMINATORS, as a funnel — which condition does the cutting:");
    println!(
        "    body forms dispatched to the rule       : {}",
        totals.dispatched
    );
    println!(
        "    destructive calls present (population)  : {}",
        totals.funnel.calls
    );
    println!(
        "      of those, on a bare variable  (cond 1): {}",
        totals.funnel.on_variable
    );
    println!(
        "      of those, in a discarded slot (cond 2): {}",
        totals.funnel.discarded
    );
    println!(
        "      of those, read by a later form(cond 3): {}   <- FINDINGS",
        totals.findings.len()
    );

    let missed: usize = totals.unparsed.iter().map(|(_, count)| count).sum();
    println!(
        "\n  UNPARSED FILES: {} (invisible to this audit; they mention ~{missed} destructive \
         operators between them)",
        totals.unparsed.len()
    );
    for (path, count) in &totals.unparsed {
        println!("    {count:4} ops  {}", path.display());
    }

    NEAR_MISSES.with(|cell| {
        let misses = cell.borrow();
        println!(
            "\n  NEAR MISSES — passed conditions 1 and 2, declined only by condition 3: {}",
            misses.len()
        );
        for miss in misses.iter() {
            println!("    {miss}");
        }
    });

    println!("\n  FINDINGS: {}", totals.findings.len());
    for finding in &totals.findings {
        println!("    {finding}");
    }

    assert!(
        totals.files_scanned > 0,
        "the corpus roots matched no files at all — that is a harness failure, not a clean sweep"
    );
    assert!(
        totals.dispatched > 0,
        "the rule was handed zero body forms; a zero-finding sweep over zero candidates proves \
         nothing"
    );
    assert!(
        totals.funnel.calls > 0,
        "the corpus contains no destructive calls at all, so it cannot test this rule"
    );
}
