//! The third-party sweep: both rules run over code they did not choose.
//!
//! `#[ignore]`d, because it reads directories that exist on the machine this
//! package was written on and nowhere else. Point it somewhere and run it:
//!
//! ```text
//! PAREDIT_AUDIT_ROOTS=/path/to/sbcl/src:/path/to/quicklisp \
//!   cargo test -p paredit-feature-lint-macro-authoring \
//!   -- --ignored --nocapture ignored_audit_
//! ```
//!
//! # The two things that make a sweep mean anything
//!
//! **The denominator.** A zero-finding sweep over zero candidates is a false
//! clean, and it looks exactly like success. This harness reports, per rule,
//! how many nodes the dispatcher actually handed it — taken from the engine's
//! own invocation counter, not from a guess — beside the finding count. A rule
//! whose head list is misspelled reports `invocations=0` here rather than a
//! reassuring `findings=0`.
//!
//! **The self-test.** [`ignored_audit_harness_detects_a_known_dirty_file`] runs
//! the same harness over a file that *must* produce findings. A previous batch
//! in this repository shipped an audit whose command line was subtly wrong;
//! every file errored out, every file reported zero findings, and the sweep read
//! as a clean success. That test is the guard, and it runs first.

#![cfg(test)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::engine_pass_tests::ENTRIES;

/// What one sweep found, per rule.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleTally {
    /// Nodes the dispatcher handed the rule: the denominator.
    pub invocations: u64,
    /// Findings the rule reported.
    pub findings: usize,
}

#[derive(Debug, Default)]
struct Sweep {
    files_scanned: usize,
    files_unparsed: usize,
    per_rule: BTreeMap<&'static str, RuleTally>,
    /// `(path, line, rule, message)` for every finding, so each can be
    /// adjudicated by hand rather than counted.
    findings: Vec<(PathBuf, usize, &'static str, String)>,
}

impl Sweep {
    /// Runs both rules over one file, folding the result in.
    ///
    /// A file that does not parse is counted rather than ignored: a sweep whose
    /// corpus silently failed to parse is the other way a false clean happens.
    fn add_file(&mut self, path: &Path) {
        let Ok(source) = std::fs::read_to_string(path) else {
            self.files_unparsed += 1;
            return;
        };
        let Ok(tree) = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp) else {
            self.files_unparsed += 1;
            return;
        };
        self.files_scanned += 1;

        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let Ok(outcome) = collect_lint_pass(
            catalog,
            &index,
            path,
            Dialect::CommonLisp,
            &tree,
            &source,
            RuleSelection::All,
            PassOptions {
                settings: None,
                measure: true,
            },
        ) else {
            self.files_unparsed += 1;
            return;
        };

        for (position, _, invocations) in outcome
            .timings
            .expect("measure: true produces timings")
            .entries()
        {
            let name = catalog.entries()[position].meta().name().as_str();
            self.per_rule.entry(name).or_default().invocations += invocations;
        }
        for finding in outcome.outcomes {
            let (report, _) = finding.into_parts();
            self.per_rule.entry(report.rule).or_default().findings += 1;
            let line = source
                .get(..report.span.start().get())
                .map_or(0, |prefix| prefix.lines().count());
            self.findings
                .push((path.to_path_buf(), line, report.rule, report.message));
        }
    }

    fn print(&self, label: &str) {
        println!(
            "\n=== {label}: {} files scanned, {} unreadable/unparsed ===",
            self.files_scanned, self.files_unparsed
        );
        for (rule, tally) in &self.per_rule {
            println!(
                "  {rule:<48} candidates={:<7} findings={}",
                tally.invocations, tally.findings
            );
        }
        for (path, line, rule, message) in &self.findings {
            println!(
                "  FINDING {}:{line} [{rule}]\n    {message}",
                path.display()
            );
        }
    }

    fn total_findings(&self) -> usize {
        self.per_rule.values().map(|tally| tally.findings).sum()
    }
}

/// Every `.lisp`/`.lsp`/`.cl` file under `root`, recursively.
fn lisp_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            lisp_files(&path, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "lisp" | "lsp" | "cl"))
        {
            out.push(path);
        }
    }
}

fn sweep_root(root: &Path) -> Sweep {
    let mut files = Vec::new();
    lisp_files(root, &mut files);
    files.sort();
    let mut sweep = Sweep::default();
    for file in &files {
        sweep.add_file(file);
    }
    sweep
}

/// **Runs first.** The harness must report findings on a file that has them.
///
/// Without this, every other assertion in this file passes just as well when
/// the harness is broken and reports zero over everything.
#[test]
#[ignore = "an audit: reads paths that exist only on the authoring machine"]
fn ignored_audit_harness_detects_a_known_dirty_file() {
    let dirty = std::env::temp_dir().join("paredit-macroauth-audit-selftest.lisp");
    std::fs::write(
        &dirty,
        "(defmacro bad (&body forms) `(progn ,@(nreverse forms)))\n\
         (let ((n 3)) (macrolet ((rep () n)) (rep)))\n",
    )
    .expect("write the self-test file");

    let mut sweep = Sweep::default();
    sweep.add_file(&dirty);
    sweep.print("self-test (known dirty)");

    assert_eq!(sweep.files_scanned, 1, "the self-test file must parse");
    assert_eq!(
        sweep.total_findings(),
        2,
        "the harness reported {} findings on a file with exactly two; every zero it reports \
         elsewhere is therefore meaningless",
        sweep.total_findings()
    );
    for (rule, tally) in &sweep.per_rule {
        assert!(tally.invocations > 0, "{rule} was never invoked");
        assert_eq!(tally.findings, 1, "{rule} should fire once here");
    }
    std::fs::remove_file(&dirty).ok();
}

/// The sweep itself. Every finding is printed for adjudication by hand.
#[test]
#[ignore = "an audit: reads paths that exist only on the authoring machine"]
fn ignored_audit_third_party_corpora() {
    let roots = std::env::var("PAREDIT_AUDIT_ROOTS")
        .expect("set PAREDIT_AUDIT_ROOTS to a ':'-separated list of directories");
    let mut total = Sweep::default();
    for root in roots.split(':').filter(|root| !root.is_empty()) {
        let sweep = sweep_root(Path::new(root));
        sweep.print(root);
        total.files_scanned += sweep.files_scanned;
        total.files_unparsed += sweep.files_unparsed;
        for (rule, tally) in sweep.per_rule {
            let entry = total.per_rule.entry(rule).or_default();
            entry.invocations += tally.invocations;
            entry.findings += tally.findings;
        }
        total.findings.extend(sweep.findings);
    }
    total.print("TOTAL");

    // The denominator assertion, which is what makes a zero mean something.
    for (rule, tally) in &total.per_rule {
        assert!(
            tally.invocations > 0,
            "{rule} scanned zero candidates across the whole corpus, so its finding count says \
             nothing about it"
        );
    }
}
