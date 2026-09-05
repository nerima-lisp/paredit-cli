//! A file that declares a package, defines things, and never enters the
//! package.
//!
//! `defpackage` creates a package; it does not select one. Only `in-package`
//! does that, by setting `*package*` for the rest of the file's load. So a file
//! shaped like
//!
//! ```lisp
//! (defpackage #:app (:use #:cl) (:export #:run))
//! (defun run () …)          ; interned into CL-USER, not APP
//! ```
//!
//! defines `CL-USER::RUN` while `APP:RUN` stays an external symbol with no
//! function binding. Everything still compiles and loads; the failure is at the
//! first call from another package, and the symbol the error names looks right.
//!
//! # Scope, and why it is this narrow
//!
//! A missing `in-package` is only *knowable* from one file when that file also
//! declares a package. Three legitimate shapes have no `in-package` and are
//! deliberately outside this rule:
//!
//! - **`package.lisp`** — a file of nothing but `defpackage` forms. Correct and
//!   universal. Handled by requiring at least one definition form in the file.
//! - **A file with no package declaration at all** — a script, a `--load`
//!   snippet, a file whose package the loader establishes. There is nothing in
//!   the file to compare against, so the rule never anchors there.
//! - **`.asd` files** — conventionally `(in-package :asdf-user)` or nothing at
//!   all, and they declare a *system*, not a package. `defsystem` is not a
//!   package declaration and is not a definition form for this rule's purposes,
//!   so an `.asd` file is silent either way.
//!
//! A **package-inferred-system** file is *in* scope and correctly so: that
//! layout requires each file to open with `(uiop:define-package :app/x …)`
//! followed by `(in-package :app/x)`, so such a file has both and stays silent.
//! What it does not have is a `define-package` with definitions and no
//! `in-package` — that shape is broken there too.
//!
//! # Further deliberate limits
//!
//! - **Any `in-package` anywhere in the file silences it, comments
//!   included.** The guard is a case-insensitive byte scan over the raw
//!   source, not a walk over forms, so it cannot tell code from a string
//!   literal or from a comment. A file whose header comment merely says
//!   "remember to `in-package` here" is therefore silent even though no
//!   `(in-package …)` form exists. That is a missed finding, which is the
//!   direction this rule errs in, and it is the price of settling the
//!   overwhelming majority of files without materializing the document.
//! - **At most one finding per file**, at the first package declaration reached
//!   as code. A file with three `defpackage`s and no `in-package` has one
//!   problem, not three.
//! - **No fix.** *Which* of the file's packages should be entered is a decision
//!   a rewrite cannot make.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::atom_text;
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, is_package_declaration, is_package_interning_definition, mentions,
};

/// The operator whose absence this rule is about. Also the byte-scan needle:
/// `cl:in-package`, `CL:IN-PACKAGE` and a bare `in-package` all contain it.
pub const IN_PACKAGE: &str = "in-package";

/// The two byte-scan needles for the operator whose *presence* this rule
/// requires. `uiop:define-package` and `cl:defpackage` both contain one of
/// them, and neither is a substring of the other, so both must be tried.
const PACKAGE_DECLARATION_NEEDLES: [&str; 2] = ["defpackage", "define-package"];

/// The substring both of [`PACKAGE_DECLARATION_NEEDLES`] contain, and the
/// cheapest question that can settle them together.
const PACKAGE_SUBSTRING: &str = "package";

/// Whether the file could contain a package declaration at all.
///
/// The positive-polarity half of the guard pair, and the one that makes the
/// walk affordable: a file that declares no package cannot produce a finding
/// and cannot contribute to the denominator, so it is settled by byte scans
/// alone. Pairing it with the negative [`IN_PACKAGE`] scan is what keeps the
/// walk off every shape except the one this rule is actually about — a file
/// that declares a package and does not enter it.
///
/// Because this rule is `WholeTree` it is dispatched on *every* file, so the
/// no-match path is the one that runs almost always and is the only one whose
/// constant matters. Both needles contain `package`, so a file that does not
/// mention `package` at all cannot match either, and asking the short question
/// first settles that file in one pass over the source instead of two. A file
/// that does mention it answers the prefilter at the first occurrence, so the
/// extra scan is paid only up to that point and never over the whole file.
#[must_use]
pub fn declares_a_package(source: &str) -> bool {
    mentions(source, PACKAGE_SUBSTRING)
        && PACKAGE_DECLARATION_NEEDLES
            .iter()
            .any(|needle| mentions(source, needle))
}

#[derive(Debug, Clone)]
pub struct DefpackageWithoutInPackageItem {
    /// The span of the package declaration. The missing `(in-package …)`
    /// belongs directly after it, which is why the finding points here rather
    /// than at one of the orphaned definitions.
    pub span: ByteSpan,
    /// The package designator exactly as written, so the reader can see which
    /// name they would be entering.
    pub package: String,
    /// How many definition forms in the file are affected.
    pub definition_count: usize,
}

impl Finding for DefpackageWithoutInPackageItem {
    fn kind(&self) -> &'static str {
        "defpackage-without-in-package"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("package={}", self.package),
            format!("definition_count={}", self.definition_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("package", json!(self.package)),
            ("definition_count", json!(self.definition_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "package {} is declared but the file never enters it; its {} definition(s) are \
             interned into whatever package is current, not into {}",
            self.package, self.definition_count, self.package
        )
    }
}

/// What one file says about its own package selection.
///
/// Read in a single evaluated-code walk, so a `defpackage` inside `'(…)` is
/// neither the file's first package declaration nor a definition — which is
/// what makes the quoted cases fall out without a second question being asked.
#[derive(Debug, Default, Clone)]
pub struct PackageEntryFacts {
    /// The first package declaration reached as code, and its designator.
    pub first_declaration: Option<(ByteSpan, String)>,
    /// How many package declarations the file makes — the denominator.
    pub declaration_count: usize,
    /// How many definition forms would be interned into the current package.
    pub definition_count: usize,
}

/// Reads one file's package-entry facts from its already-materialized root
/// view.
///
/// Takes the root view rather than the [`SyntaxTree`] on purpose:
/// `SyntaxTree::root_view` deep-materializes the whole document and is
/// uncached, so a function that called it itself would rebuild the document
/// once per caller. The dispatcher already builds the root view for every
/// file, and hands it to `WholeTree` rules, so the rule pays nothing for it.
///
/// Called only after both byte-scan guards have passed — the file declares a
/// package and does not enter one — so no file outside this rule's subject
/// ever pays for the walk.
#[must_use]
pub fn read_package_entry_facts(root: &ExpressionView) -> PackageEntryFacts {
    let mut facts = PackageEntryFacts::default();
    for_each_evaluated_subview(root, |view| {
        if is_package_declaration(view) {
            facts.declaration_count += 1;
            if facts.first_declaration.is_none() {
                let package = view
                    .children
                    .get(1)
                    .and_then(atom_text)
                    .unwrap_or("<unnamed>")
                    .to_owned();
                facts.first_declaration = Some((view.span, package));
            }
        } else if is_package_interning_definition(view) {
            facts.definition_count += 1;
        }
    });
    facts
}

/// The finding a set of facts implies, if any.
#[must_use]
fn finding_from(facts: PackageEntryFacts) -> Option<DefpackageWithoutInPackageItem> {
    let definition_count = facts.definition_count;
    if definition_count == 0 {
        return None;
    }
    let (span, package) = facts.first_declaration?;
    Some(DefpackageWithoutInPackageItem {
        span,
        package,
        definition_count,
    })
}

/// The file's finding, if it has one.
///
/// One or none: see the module docs on why a file with three `defpackage`s and
/// no `in-package` has one problem rather than three. This is a pure function
/// of the file, which is why the rule can be dispatched once per file rather
/// than once per declaration.
///
/// The two byte scans below are the whole cost model. They are ordered
/// cheapest-discriminator-first only in the sense that both are byte scans;
/// what matters is that between them they exclude every file shape except a
/// file that declares a package and never enters it.
#[must_use]
pub fn examine_file(source: &str, root: &ExpressionView) -> Option<DefpackageWithoutInPackageItem> {
    if !declares_a_package(source) {
        return None;
    }
    if mentions(source, IN_PACKAGE) {
        return None;
    }
    finding_from(read_package_entry_facts(root))
}

/// Collects the file's finding, if any, with the number of package
/// declarations scanned as the denominator beside it.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_defpackage_without_in_package_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefpackageWithoutInPackageItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("package_declaration_count", json!(0))],
        ));
    }

    let source = tree.source();
    // Materialized once and shared by both reads below. `root_view` rebuilds
    // the whole document on every call, so calling it twice here would double
    // the report's cost for nothing.
    let root = tree.root_view();
    let violations = examine_file(source, &root).map_or_else(Vec::new, |item| vec![item]);
    // The denominator is every declaration in the file, which the `in-package`
    // guard inside `examine_file` may have skipped past. The standalone report
    // — unlike the rule — is expected to publish a denominator even when it
    // finds nothing, so it is read separately, under the same positive-polarity
    // guard.
    let declaration_count = if declares_a_package(source) {
        read_package_entry_facts(&root).declaration_count
    } else {
        0
    };

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        source,
        violations,
        vec![("package_declaration_count", json!(declaration_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefpackageWithoutInPackageItem> {
        // `parse_with_dialect`, never the legacy `SyntaxTree::parse`.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defpackage_without_in_package_report(
            Path::new("app.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build defpackage-without-in-package report")
    }

    fn findings(input: &str) -> Vec<DefpackageWithoutInPackageItem> {
        report(input).findings
    }

    fn declarations(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "package_declaration_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("package_declaration_count in the summary")
    }

    // --- positive

    #[test]
    fn flags_a_file_that_declares_a_package_defines_things_and_never_enters_it() {
        let violations =
            findings("(defpackage #:app\n  (:use #:cl)\n  (:export #:run))\n\n(defun run () 1)\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].package, "#:app");
        assert_eq!(violations[0].definition_count, 1);
    }

    #[test]
    fn flags_the_uiop_define_package_spelling_too() {
        let violations = findings("(uiop:define-package :app/x (:use :cl))\n(defvar *x* 1)\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].package, ":app/x");
    }

    #[test]
    fn reports_once_at_the_first_declaration_however_many_there_are() {
        let violations =
            findings("(defpackage :a (:use :cl))\n(defpackage :b (:use :cl))\n(defun f () 1)\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].package, ":a");
        assert_eq!(
            declarations("(defpackage :a)\n(defpackage :b)\n(defun f () 1)\n"),
            2
        );
    }

    #[test]
    fn counts_every_affected_definition_form() {
        let violations = findings(
            "(defpackage :app)\n\
             (defun f () 1)\n\
             (defmacro m () 1)\n\
             (defvar *v* 1)\n\
             (defclass c () ())\n\
             (define-condition e (error) ())\n",
        );
        assert_eq!(violations[0].definition_count, 5);
    }

    // --- near-miss negatives

    #[test]
    fn does_not_flag_a_file_that_enters_its_package() {
        let violations =
            findings("(defpackage :app (:use :cl))\n(in-package :app)\n(defun run () 1)\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_qualified_or_upcased_in_package() {
        for entry in [
            "(cl:in-package :app)",
            "(CL:IN-PACKAGE :APP)",
            "(IN-PACKAGE :app)",
        ] {
            let violations = findings(&format!(
                "(defpackage :app (:use :cl))\n{entry}\n(defun run () 1)\n"
            ));
            assert!(violations.is_empty(), "flagged despite `{entry}`");
        }
    }

    /// The universal `package.lisp`: declarations and nothing else.
    #[test]
    fn does_not_flag_a_package_definition_file_with_no_definitions() {
        let violations = findings(
            "(defpackage #:app\n\
             \x20 (:use #:cl)\n\
             \x20 (:export #:run #:stop))\n\
             \n\
             (defpackage #:app-test\n\
             \x20 (:use #:cl #:app))\n",
        );
        assert!(violations.is_empty());
        assert_eq!(declarations("(defpackage :a)\n(defpackage :b)\n"), 2);
    }

    #[test]
    fn does_not_flag_a_file_with_definitions_but_no_package_declaration() {
        // Nothing in the file to compare against: the package may be
        // established by the loader, by a script header, or by the REPL.
        let violations = findings("(defun run () 1)\n(defvar *x* 2)\n");
        assert!(violations.is_empty());
        assert_eq!(declarations("(defun run () 1)\n"), 0);
    }

    /// A realistic, correct `.asd` file. It declares a system, not a package,
    /// and conventionally has no `in-package` at all.
    #[test]
    fn does_not_flag_a_realistic_asd_file() {
        let violations = findings(
            "(defsystem \"app\"\n\
             \x20 :version \"1.0.0\"\n\
             \x20 :author \"Someone\"\n\
             \x20 :license \"MIT\"\n\
             \x20 :depends-on (\"alexandria\")\n\
             \x20 :serial t\n\
             \x20 :components ((:module \"src\"\n\
             \x20                :components ((:file \"package\")\n\
             \x20                             (:file \"app\")))))\n",
        );
        assert!(violations.is_empty());
    }

    /// The package-inferred-system layout, written correctly.
    #[test]
    fn does_not_flag_a_correct_package_inferred_system_file() {
        let violations = findings(
            "(uiop:define-package :app/util\n\
             \x20 (:use :cl)\n\
             \x20 (:export #:clamp))\n\
             (in-package :app/util)\n\
             \n\
             (defun clamp (x lo hi) (max lo (min hi x)))\n",
        );
        assert!(violations.is_empty());
    }

    /// FP-5, from the adversarial review. A symbol read with an explicit
    /// package prefix is interned exactly where the prefix says; `*package*` is
    /// irrelevant. This file is correct and loads fine, and the old wording
    /// ("interned into whatever package is current") was simply false about it.
    #[test]
    fn does_not_flag_definitions_written_with_an_explicit_package_qualifier() {
        let violations = findings(
            "(defpackage #:tiny-util\n\
             \x20 (:use #:cl)\n\
             \x20 (:export #:square #:cube))\n\
             \n\
             (defun tiny-util:square (x) (* x x))\n\
             (defun tiny-util:cube (x) (* x x x))\n\
             (defvar tiny-util::*calls* 0)\n",
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn a_mix_of_qualified_and_unqualified_definitions_counts_only_the_unqualified() {
        let violations = findings("(defpackage :app)\n(defun app:run () 1)\n(defun helper () 2)\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].definition_count, 1);
    }

    /// `eval-when` is ordinary evaluated code, so the walk descends into it and
    /// the declaration inside is found like any other.
    #[test]
    fn a_declaration_inside_eval_when_still_anchors_the_finding() {
        let violations = findings(
            "(eval-when (:compile-toplevel :load-toplevel :execute)\n\
             \x20 (defpackage :app (:use :cl)))\n\
             (defun run () 1)\n",
        );
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_defsystem_is_not_a_definition_that_needs_a_package() {
        // A `.asd` that also declares a package for its own use, with no
        // definitions: still nothing to intern.
        let violations =
            findings("(defpackage :app-system (:use :cl :asdf))\n(defsystem \"app\")\n");
        assert!(violations.is_empty());
    }

    // --- quote/quasiquote negatives (the five shapes)

    #[test]
    fn a_hard_quoted_declaration_is_list_data_and_anchors_nothing() {
        let violations = findings("'(defpackage :app)\n(defun f () 1)\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_list_data_and_anchors_nothing() {
        let violations = findings("(quote (defpackage :app))\n(defun f () 1)\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_still_list_data() {
        let violations = findings("'(a ,(defpackage :app))\n(defun f () 1)\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_list_data() {
        let violations = findings("`(defpackage :app)\n(defun f () 1)\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn an_unquoted_declaration_inside_a_backquote_is_code_and_anchors_the_finding() {
        let violations = findings("`(a ,(defpackage :app))\n(defun f () 1)\n");
        assert_eq!(violations.len(), 1);
    }

    /// A quoted definition is data too, so it cannot be one of the definitions
    /// the finding counts.
    #[test]
    fn a_quoted_definition_is_not_counted() {
        let violations = findings("(defpackage :app)\n'(defun f () 1)\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn a_definition_template_inside_a_macro_body_is_data_and_is_not_counted() {
        let violations = findings("(defpackage :app)\n`(defun ,name () 1)\n");
        assert!(violations.is_empty());
    }

    // --- string-literal negative

    #[test]
    fn a_declaration_inside_a_string_literal_is_one_atom_and_is_not_a_form() {
        let violations = findings("(format t \"(defpackage :app)\")\n");
        assert!(violations.is_empty());
        assert_eq!(declarations("(format t \"(defpackage :app)\")\n"), 0);
    }

    /// The byte-scan guard cannot tell a string from code, so a mention inside
    /// one silences the file. Documented, and the direction this rule errs in.
    #[test]
    fn an_in_package_mentioned_only_in_a_string_silences_the_file() {
        let violations =
            findings("(defpackage :app)\n(defun f () \"call (in-package :app) first\")\n");
        assert!(violations.is_empty());
    }

    // --- envelope

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defpackage :app)\n(defn f [] 1)", Dialect::Clojure)
                .expect("parse");
        let report = build_defpackage_without_in_package_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("package_declaration_count", json!(0))]
        );
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report("\n(defpackage :app)\n(defun f () 1)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "defpackage-without-in-package");
        assert_eq!(
            finding.json_fields(),
            vec![("package", json!(":app")), ("definition_count", json!(1))]
        );
        assert!(finding.message().contains("never enters it"));
    }

    #[test]
    fn the_finding_is_anchored_at_the_first_declaration_not_the_second() {
        let input = "(defpackage :a)\n(defpackage :b)\n(defun f () 1)\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let item = examine_file(input, &root).expect("a finding");
        assert_eq!(item.span, root.children[0].span);
        assert_ne!(item.span, root.children[1].span);
    }

    // --- the guard pair

    /// The positive-polarity guard. Under `WholeTree` the rule is dispatched
    /// on every file, so this is what keeps the walk off the files that have
    /// nothing to do with it.
    #[test]
    fn a_file_that_declares_no_package_is_settled_without_a_walk() {
        for source in [
            "",
            "(defun f () 1)\n",
            "(in-package :app)\n(defun f () 1)\n",
        ] {
            assert!(!declares_a_package(source), "byte scan fired on `{source}`");
            let tree =
                SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse input");
            assert!(examine_file(source, &tree.root_view()).is_none());
        }
    }

    #[test]
    fn the_positive_guard_matches_both_spellings_in_any_case() {
        for source in [
            "(defpackage :a)",
            "(CL:DEFPACKAGE :a)",
            "(uiop:define-package :a)",
            "(DEFINE-PACKAGE :a)",
        ] {
            assert!(declares_a_package(source), "byte scan missed `{source}`");
        }
    }

    /// The `package` prefilter is an optimisation and must not be observable.
    ///
    /// It is only sound because both needles contain `package`; a third needle
    /// that did not would be silently dropped by it. This pins the property
    /// rather than the current needle list, so adding such a needle fails here
    /// instead of in a missed finding.
    #[test]
    fn every_declaration_needle_contains_the_prefiltered_substring() {
        for needle in PACKAGE_DECLARATION_NEEDLES {
            assert!(
                needle.contains(PACKAGE_SUBSTRING),
                "`{needle}` does not contain `{PACKAGE_SUBSTRING}`, so the prefilter in \
                 `declares_a_package` would discard it"
            );
        }
    }

    /// A file that mentions `package` only in a shape neither needle matches
    /// still has to be answered by the full scans, not by the prefilter.
    #[test]
    fn the_prefilter_does_not_answer_for_the_full_scans() {
        for source in [
            "(in-package :app)",
            "(find-package :app)",
            "(defun package-name-of (x) x)",
        ] {
            assert!(
                mentions(source, PACKAGE_SUBSTRING),
                "the prefilter should not settle `{source}`"
            );
            assert!(
                !declares_a_package(source),
                "`{source}` is not a package declaration"
            );
        }
    }
}
