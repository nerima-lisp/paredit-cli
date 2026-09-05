//! A file that leaves a package and later comes back to it.
//!
//! `in-package` is a *reader-time* switch: it sets `*package*` for everything
//! the reader sees after it, so a file shaped like
//!
//! ```lisp
//! (in-package :app)
//! (defun start () …)        ; APP::START
//! (in-package :app-test)
//! (defun check () …)        ; APP-TEST::CHECK
//! (in-package :app)
//! (defun stop () …)         ; APP::STOP
//! ```
//!
//! has two disjoint regions of `APP` with a region of `APP-TEST` wedged between
//! them. Nothing here fails to compile. What it costs is that `APP`'s
//! definitions no longer sit together, that a symbol spelled the same way in
//! the middle region is a *different symbol* from the one in either outer
//! region, and that the load order of the file's own definitions now depends on
//! where the switches fall rather than on where the definitions do.
//!
//! # Scope, and why it is this narrow
//!
//! - **Top-level switches only.** CLHS 11.1.2.2 requires `in-package` at top
//!   level for its compile-time effect. One nested inside `eval-when`, `progn`
//!   or a function body does not switch the reader for the following top-level
//!   forms, so it is not part of the chain and counting it would report a
//!   switch that never happened. See
//!   [`crate::support::top_level_in_package_chain`].
//! - **Ambient packages are never the subject.** Re-entering `CL-USER` — the
//!   `(in-package :cl-user)` / `defpackage` / `(in-package :app)` /
//!   `(in-package :cl-user)` / `defpackage` / `(in-package :app-test)` shape a
//!   single-file library is normally written in — is correct and universal.
//!   `CL`, `COMMON-LISP`, `CL-USER`, `COMMON-LISP-USER` and `KEYWORD` are all
//!   exempt; that is [`crate::support::AMBIENT_PACKAGES`], the same set
//!   `paredit_feature_project_analysis::undefined_package_report` declares.
//! - **Adjacent repetition is not circularity.** `(in-package :a)` twice in a
//!   row is redundant; nothing was left and re-entered. Consecutive runs are
//!   collapsed before the search, so that shape is silent here. A rule about
//!   redundant repetition would be a different rule.
//! - **One finding per re-entered package**, at its *first* re-entry. A file
//!   that alternates `A B A B A` has split two packages, not three; reporting
//!   every switch after the first would scale the noise with the alternation
//!   rather than with the number of packages actually split.
//! - **Reader-conditional switches are invisible.** `#+sbcl (in-package :a)`
//!   parses as a single opaque atom in this codebase, so it never enters the
//!   chain. The resulting chain is build-dependent and cannot be read
//!   statically; erring towards the missed finding is deliberate.
//! - **No fix.** Merging two regions of one package means *moving forms*, and
//!   which region should absorb the other — and whether the middle region
//!   depends on the first having been read already — are questions a rewrite
//!   cannot answer.
//!
//! Scope: Common Lisp only. `in-package` is the CL package system's operator;
//! Clojure's `ns` and Racket's `module` have no equivalent re-entry.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    IN_PACKAGE, InPackageEntry, is_ambient_package, is_unevaluated_at, package_designator_name,
    root_child_index_containing, top_level_in_package_chain, top_level_in_package_chain_through,
};

#[derive(Debug, Clone)]
pub struct CircularInPackageChainItem {
    /// The span of the re-entering `(in-package …)` form — the switch that
    /// closes the loop, which is the one a reader has to move or delete.
    pub span: ByteSpan,
    /// The re-entered package's designator exactly as this form writes it, so
    /// the message echoes the source rather than the normalized spelling.
    pub package: String,
    /// The packages entered between the first entry and this re-entry, in
    /// document order and without consecutive repeats, written as their own
    /// forms write them. Never empty: a re-entry with nothing in between is
    /// adjacent repetition, which this rule does not report.
    pub intervening: Vec<String>,
}

impl Finding for CircularInPackageChainItem {
    fn kind(&self) -> &'static str {
        "package-circular-in-package-chain"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("package={}", self.package),
            format!("intervening={}", self.intervening.join(",")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("package", json!(self.package)),
            ("intervening", json!(self.intervening)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "package {} is entered again after the file left it for {}; its top-level forms are \
             split into two regions that read in different packages",
            self.package,
            self.intervening.join(", ")
        )
    }
}

/// The chain with consecutive repeats collapsed.
///
/// `A A B A` becomes `A B A`. Keeping the *first* entry of each run is what
/// makes "the package this switch left" well defined, and dropping the rest is
/// what keeps adjacent repetition — which is redundant, not circular — out of
/// the search.
fn collapse_runs(chain: &[InPackageEntry]) -> Vec<&InPackageEntry> {
    let mut collapsed: Vec<&InPackageEntry> = Vec::with_capacity(chain.len());
    for entry in chain {
        if collapsed
            .last()
            .is_some_and(|previous| previous.package == entry.package)
        {
            continue;
        }
        collapsed.push(entry);
    }
    collapsed
}

/// Every package this file leaves and comes back to, reported at the switch
/// that comes back.
///
/// Pure over the chain, so the whole detection is testable without a tree.
#[must_use]
pub fn chain_reentries(chain: &[InPackageEntry]) -> Vec<CircularInPackageChainItem> {
    let collapsed = collapse_runs(chain);
    let mut items = Vec::new();
    let mut already_reported: Vec<&str> = Vec::new();

    for (position, entry) in collapsed.iter().enumerate() {
        // An ambient package is bounced through, not split: see the module
        // docs. Checked here rather than while building the chain, because a
        // *bounce* through `CL-USER` still separates two regions of a
        // non-ambient package, and dropping it from the chain would hide that.
        if is_ambient_package(&entry.package) {
            continue;
        }
        if already_reported.contains(&entry.package.as_str()) {
            continue;
        }
        let Some(first) = collapsed[..position]
            .iter()
            .position(|earlier| earlier.package == entry.package)
        else {
            continue;
        };
        // Non-empty by construction: `collapse_runs` guarantees no two adjacent
        // entries share a package, so `first < position` implies at least one
        // different package in between.
        let intervening: Vec<String> = collapsed[first + 1..position]
            .iter()
            .map(|between| between.written.clone())
            .collect();
        already_reported.push(&entry.package);
        items.push(CircularInPackageChainItem {
            span: entry.span,
            package: entry.written.clone(),
            intervening,
        });
    }
    items
}

/// Whether `view` is an `(in-package <designator>)` list, and what it
/// designates in the normalized spelling.
///
/// Reads only the node it is given, so it is the cheap half of the rule's
/// rejection path: everything the dispatcher hands over that is not a live,
/// non-ambient package switch is settled without looking at the rest of the
/// file.
#[must_use]
pub fn switched_package(view: &ExpressionView) -> Option<String> {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, IN_PACKAGE)) {
        return None;
    }
    package_designator_name(atom_text(view.children.get(1)?)?)
}

/// The finding this `in-package` form earns, if it is the one that closes a
/// loop.
///
/// Shared with the lint suite's rule, which reaches every `in-package` node
/// through the single dispatch pass instead of walking the tree again.
///
/// # The three early returns are fast paths, not correctness guards
///
/// Stated plainly because mutation testing says so: deleting the ambient check,
/// the quote check, or the top-level check from *this function* changes no
/// finding on any of the 84 tests. Each is subsumed by something later:
///
/// - the ambient check by [`chain_reentries`]'s own, which is the one that
///   matters and which does fail tests when removed;
/// - the quote check by [`top_level_in_package_chain_through`]'s, so a quoted
///   form's span never enters the chain and the final `find` cannot match it;
/// - the top-level check by the same `find`, since a nested form's span is
///   never a chain entry's span.
///
/// They earn their place on cost rather than on correctness. Each one settles a
/// node *before* the scan below it, and the scan is the only part of this
/// function that is not O(1). Keeping them means the ordinary
/// `(in-package :cl-user)` never reads the file at all. The correctness of the
/// answer does not rest on them, and the module's tests deliberately do not
/// pretend otherwise.
///
/// # Cost
///
/// The rejection path is ordered cheapest-first:
///
/// 1. node-local shape — reads only the view the dispatcher already built;
/// 2. the ambient set — five `&str` comparisons, and the common `in-package`
///    in a single-file library;
/// 3. the quote question — costs the enclosing top-level form, never the file;
/// 4. locating this form at the top level — a binary search over root children,
///    which also rejects a *nested* `in-package` (one inside `eval-when` or a
///    function body is not a reader switch);
/// 5. only then the scan, and only of the top level *up to this form*.
///
/// Step 5's bound is what keeps the ordinary file free: an implementation file
/// has one `in-package`, at the top, so the scan stops after a form or two
/// rather than walking every definition below it. Nothing after this form can
/// change whether this form re-enters something — see
/// [`top_level_in_package_chain_through`]. That bound *is* load-bearing, and
/// removing it fails `the_ordinary_one_switch_file_costs_almost_nothing`.
#[must_use]
pub fn examine_in_package(
    tree: &SyntaxTree,
    view: &ExpressionView,
) -> Option<CircularInPackageChainItem> {
    let package = switched_package(view)?;
    // Fast path: `chain_reentries` would decline this too. See the note above.
    if is_ambient_package(&package) {
        return None;
    }
    // Fast path: the dispatcher hands a rule every head-matched node, quoted or
    // not, and the chain scan declines quoted forms as well.
    if is_unevaluated_at(tree, view.span) {
        return None;
    }
    let root_index = root_child_index_containing(tree, view.span)?;
    // Fast path: a nested `in-package` is contained by a top-level form that is
    // not itself, so its span is never a chain entry's and the `find` below
    // would miss anyway. It switches nothing the reader will act on.
    if tree
        .select_path(&SexprPath::root_child(root_index))
        .ok()?
        .span()
        != view.span
    {
        return None;
    }
    let chain = top_level_in_package_chain_through(tree, root_index);
    chain_reentries(&chain)
        .into_iter()
        .find(|item| item.span == view.span)
}

/// Collects every re-entered package in one file, with the number of top-level
/// package switches scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_circular_in_package_chain_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CircularInPackageChainItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("in_package_switch_count", json!(0))],
        ));
    }

    // Built once and shared by both the denominator and the search. This is
    // the report's whole cost: no `root_view`, and no `ExpressionView` for any
    // form that is not an `in-package`.
    let chain = top_level_in_package_chain(tree);
    let switch_count = chain.len();
    let violations = chain_reentries(&chain);

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("in_package_switch_count", json!(switch_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::Path as SexprPath;

    fn report(input: &str) -> FileFindings<CircularInPackageChainItem> {
        // `parse_with_dialect`, never the legacy `SyntaxTree::parse`.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_circular_in_package_chain_report(Path::new("app.lisp"), Dialect::CommonLisp, &tree)
            .expect("build circular in-package chain report")
    }

    fn findings(input: &str) -> Vec<CircularInPackageChainItem> {
        report(input).findings
    }

    fn packages(input: &str) -> Vec<String> {
        findings(input)
            .into_iter()
            .map(|item| item.package)
            .collect()
    }

    fn switches(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "in_package_switch_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("in_package_switch_count in the summary")
    }

    /// Drives `examine_in_package` at the top-level form at `root_index`,
    /// which is what the registered rule does with the node the dispatcher
    /// hands it.
    fn examine_at(input: &str, root_index: usize) -> Option<CircularInPackageChainItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        let view = tree
            .select_path(&SexprPath::root_child(root_index))
            .expect("the form")
            .view();
        examine_in_package(&tree, &view)
    }

    // --- positive

    #[test]
    fn flags_a_package_that_is_left_and_re_entered() {
        let violations = findings(
            "(in-package :app)\n\
             (defun start () 1)\n\
             (in-package :app-test)\n\
             (defun check () 2)\n\
             (in-package :app)\n\
             (defun stop () 3)\n",
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].package, ":app");
        assert_eq!(violations[0].intervening, vec![":app-test".to_owned()]);
    }

    #[test]
    fn the_finding_is_anchored_at_the_re_entry_not_at_the_first_entry() {
        let input = "(in-package :app)\n(in-package :other)\n(in-package :app)\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let violations = findings(input);
        let third = tree
            .select_path(&SexprPath::root_child(2))
            .expect("the third form")
            .span();
        let first = tree
            .select_path(&SexprPath::root_child(0))
            .expect("the first form")
            .span();
        assert_eq!(violations[0].span, third);
        assert_ne!(violations[0].span, first);
    }

    #[test]
    fn folds_designator_spellings_so_a_re_entry_is_seen_through_them() {
        let violations =
            findings("(in-package #:app)\n(in-package :other)\n(in-package \"APP\")\n");
        assert_eq!(violations.len(), 1);
        // The message echoes the re-entry's own spelling, not the first one's.
        assert_eq!(violations[0].package, "\"APP\"");
    }

    #[test]
    fn reports_each_re_entered_package_once_at_its_first_re_entry() {
        let input = "(in-package :a)\n\
                     (in-package :b)\n\
                     (in-package :a)\n\
                     (in-package :b)\n\
                     (in-package :a)\n";
        assert_eq!(packages(input), vec![":a".to_owned(), ":b".to_owned()]);
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        // `:a`'s finding is the third form, `:b`'s is the fourth — the first
        // re-entry of each, not the last.
        let violations = findings(input);
        assert_eq!(
            violations[0].span,
            tree.select_path(&SexprPath::root_child(2))
                .expect("form")
                .span()
        );
        assert_eq!(
            violations[1].span,
            tree.select_path(&SexprPath::root_child(3))
                .expect("form")
                .span()
        );
    }

    #[test]
    fn records_every_package_visited_in_between() {
        let violations =
            findings("(in-package :a)\n(in-package :b)\n(in-package :c)\n(in-package :a)\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].intervening,
            vec![":b".to_owned(), ":c".to_owned()]
        );
    }

    #[test]
    fn a_bounce_through_an_ambient_package_still_splits_the_one_that_is_split() {
        // `:cl-user` is exempt as a *subject*, but it is still a real switch:
        // `:app`'s two regions are genuinely separated by it.
        let violations = findings("(in-package :app)\n(in-package :cl-user)\n(in-package :app)\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].package, ":app");
        assert_eq!(violations[0].intervening, vec![":cl-user".to_owned()]);
    }

    // --- near-miss negatives

    #[test]
    fn does_not_flag_a_file_that_enters_one_package_and_stays() {
        let violations = findings("(in-package :app)\n(defun f () 1)\n(defun g () 2)\n");
        assert!(violations.is_empty());
        assert_eq!(switches("(in-package :app)\n(defun f () 1)\n"), 1);
    }

    #[test]
    fn does_not_flag_a_forward_only_chain() {
        let violations = findings("(in-package :a)\n(in-package :b)\n(in-package :c)\n");
        assert!(violations.is_empty());
        assert_eq!(
            switches("(in-package :a)\n(in-package :b)\n(in-package :c)\n"),
            3
        );
    }

    /// The near miss that matters most: adjacent repetition is redundant, not
    /// circular. Nothing was left, so nothing was re-entered.
    #[test]
    fn does_not_flag_the_same_package_entered_twice_in_a_row() {
        assert!(findings("(in-package :app)\n(in-package :app)\n").is_empty());
        assert!(
            findings("(in-package :app)\n(in-package #:app)\n(in-package \"APP\")\n").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_run_that_repeats_before_moving_on() {
        // `A A B` — the repeat is adjacent, and `B` never comes back to `A`.
        assert!(findings("(in-package :a)\n(in-package :a)\n(in-package :b)\n").is_empty());
    }

    /// The realistic single-file library: bounce out to `CL-USER` to declare
    /// the next package, then enter it. `CL-USER` is re-entered and must stay
    /// silent.
    #[test]
    fn does_not_flag_a_realistic_cl_user_bounce_between_two_package_declarations() {
        let violations = findings(
            "(in-package :cl-user)\n\
             (defpackage :app (:use :cl) (:export #:run))\n\
             (in-package :app)\n\
             (defun run () 1)\n\
             (in-package :cl-user)\n\
             (defpackage :app/test (:use :cl :app))\n\
             (in-package :app/test)\n\
             (defun test-run () (run))\n",
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn no_ambient_package_is_ever_the_subject() {
        for ambient in [
            ":cl",
            ":common-lisp",
            ":cl-user",
            ":common-lisp-user",
            ":keyword",
        ] {
            let source =
                format!("(in-package {ambient})\n(in-package :app)\n(in-package {ambient})\n");
            assert!(
                findings(&source).is_empty(),
                "{ambient} was reported as re-entered"
            );
        }
    }

    #[test]
    fn a_nested_in_package_is_not_a_switch_and_cannot_close_a_loop() {
        let violations = findings(
            "(in-package :app)\n\
             (in-package :other)\n\
             (eval-when (:execute) (in-package :app))\n",
        );
        assert!(violations.is_empty());
        assert_eq!(
            switches("(in-package :app)\n(eval-when (:execute) (in-package :b))\n"),
            1
        );
    }

    #[test]
    fn a_malformed_in_package_is_skipped_rather_than_guessed_at() {
        assert!(findings("(in-package :a)\n(in-package :b)\n(in-package)\n").is_empty());
    }

    /// `#+sbcl (in-package :a)` is one opaque atom here, so the chain it would
    /// create is invisible. A missed finding, deliberately.
    #[test]
    fn a_reader_conditional_switch_is_not_read_as_a_switch() {
        assert!(findings("(in-package :a)\n(in-package :b)\n#+sbcl (in-package :a)\n").is_empty());
    }

    // --- quote/quasiquote negatives (the five shapes)

    #[test]
    fn a_hard_quoted_switch_is_list_data_and_closes_no_loop() {
        assert!(findings("(in-package :a)\n(in-package :b)\n'(in-package :a)\n").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_list_data_and_closes_no_loop() {
        assert!(findings("(in-package :a)\n(in-package :b)\n(quote (in-package :a))\n").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_list_data() {
        assert!(findings("(in-package :a)\n(in-package :b)\n`(in-package :a)\n").is_empty());
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn a_comma_inside_a_hard_quote_is_still_list_data() {
        assert!(findings("(in-package :a)\n(in-package :b)\n'(x ,(in-package :a))\n").is_empty());
    }

    /// The fifth shape. The unquoted form is code, but the *top-level* form is
    /// the backquoted list, so it is still not a reader switch — and the rule
    /// must not report it either way.
    #[test]
    fn an_unquoted_switch_inside_a_backquote_is_not_a_top_level_switch() {
        assert!(findings("(in-package :a)\n(in-package :b)\n`(x ,(in-package :a))\n").is_empty());
        assert_eq!(
            switches("(in-package :a)\n`(x ,(in-package :a))\n"),
            1,
            "only the real switch counts"
        );
    }

    #[test]
    fn a_quoted_switch_in_the_middle_does_not_separate_two_regions() {
        // Without the quote guard this would read as `A → B → A` and fire.
        assert!(findings("(in-package :a)\n'(in-package :b)\n(in-package :a)\n").is_empty());
    }

    // --- string-literal negative

    #[test]
    fn a_switch_inside_a_string_literal_is_one_atom_and_is_not_a_form() {
        let source = "(in-package :a)\n\
                      (in-package :b)\n\
                      (defun doc () \"call (in-package :a) first\")\n";
        assert!(findings(source).is_empty());
        assert_eq!(switches(source), 2);
    }

    // --- examine_in_package, as the rule calls it

    #[test]
    fn examine_answers_only_at_the_re_entering_node() {
        let input = "(in-package :a)\n(in-package :b)\n(in-package :a)\n";
        assert!(examine_at(input, 0).is_none(), "the first entry");
        assert!(examine_at(input, 1).is_none(), "the intervening switch");
        assert!(examine_at(input, 2).is_some(), "the re-entry");
    }

    #[test]
    fn examine_rejects_a_node_that_is_not_an_in_package_form() {
        let input = "(defun f () 1)\n";
        assert!(examine_at(input, 0).is_none());
    }

    #[test]
    fn examine_rejects_an_ambient_switch_without_reading_the_file() {
        let input = "(in-package :cl-user)\n(in-package :app)\n(in-package :cl-user)\n";
        assert!(examine_at(input, 2).is_none());
    }

    #[test]
    fn switched_package_reads_only_the_node_it_is_given() {
        let tree = SyntaxTree::parse_with_dialect("(cl:in-package #:App)", Dialect::CommonLisp)
            .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert_eq!(switched_package(&view).as_deref(), Some("app"));

        let tree = SyntaxTree::parse_with_dialect("(defpackage :app)", Dialect::CommonLisp)
            .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert_eq!(switched_package(&view), None);
    }

    // --- the pure chain search

    #[test]
    fn the_search_is_a_pure_function_of_the_chain() {
        let tree = SyntaxTree::parse_with_dialect(
            "(in-package :a)\n(in-package :b)\n(in-package :a)\n",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let chain = top_level_in_package_chain(&tree);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain_reentries(&chain).len(), 1);
        assert!(chain_reentries(&[]).is_empty());
        assert!(chain_reentries(&chain[..2]).is_empty());
    }

    // --- envelope

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(in-package :a)\n(in-package :b)\n(in-package :a)\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_circular_in_package_chain_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("in_package_switch_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(in-package :app)\n").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report("(in-package :a)\n(in-package :b)\n(in-package :a)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "package-circular-in-package-chain");
        assert_eq!(
            finding.json_fields(),
            vec![("package", json!(":a")), ("intervening", json!([":b"]))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["package=:a".to_owned(), "intervening=:b".to_owned()]
        );
        assert!(finding.message().contains("entered again"));
        assert!(finding.message().contains(":b"));
    }

    #[test]
    fn the_summary_counts_every_switch_scanned_not_only_the_flagged_ones() {
        let report = report("(in-package :a)\n(in-package :b)\n(in-package :a)\n");
        assert_eq!(report.summary, vec![("in_package_switch_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_file_with_no_switch_at_all_reports_a_zero_denominator() {
        assert_eq!(switches("(defun f () 1)\n"), 0);
        assert!(findings("(defun f () 1)\n").is_empty());
    }
}
