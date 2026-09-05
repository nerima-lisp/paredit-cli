//! A released ASDF system with no `:version`.
//!
//! `:version` is what makes a system's identity comparable over time. It is
//! what `asdf:component-version` returns, what a dependant's
//! `(:version "dep" "1.2")` floor is compared against, and what Quicklisp and
//! release scripts read to decide what they are shipping. A system without one
//! answers `NIL` to all of them.
//!
//! # This rule is opinionated, and is tagged as such
//!
//! It carries [`RuleTag::Pedantic`], so the `recommended` and `minimal` presets
//! exclude it and only `pedantic` turns it on. That is a conclusion from
//! measurement rather than taste: run over a local Quicklisp checkout — 719
//! files, 104 primary systems — an earlier version of this rule produced 36
//! findings, **every one of them on correct code**. Twenty-six were test and
//! example systems (now exempt, see [`is_exempt_system_name`]). The remaining
//! ten were shipped, widely-depended-on libraries whose maintainers omit
//! `:version` deliberately: `babel`, `cffi` and four of its subsystems, `uffi`,
//! `cl-ppcre-unicode`, `trivial-features`, `trivial-garbage`. A ~10% firing
//! rate on expert-authored code is a convention this rule is entitled to have
//! an opinion about, but not one a default-on suite should assert.
//!
//! `cffi.asd` is worth reading on this point: it omits `:version` and ships a
//! `version-satisfies` method so that every version floor naming it succeeds
//! anyway. That is why the finding's wording states only that
//! `component-version` is `NIL` and claims nothing about what a floor can do —
//! a rule may not assert a consequence the code under it is free to override.
//!
//! [`RuleTag::Pedantic`]: paredit_core_lint_engine::model::RuleTag::Pedantic
//!
//! # What this rule does *not* attempt
//!
//! - **Support systems are exempt** — secondary systems (`"app/tests"`) and
//!   systems whose name carries a `test`/`example`/`benchmark`-family segment.
//!   See [`is_exempt_system_name`]; they are excluded from the denominator as
//!   well as from the findings, because putting them in it would understate the
//!   rate for the systems this rule does have an opinion about.
//! - **The version's shape is not checked.** `:version "1.0"`,
//!   `:version (:read-file-form "version.sexp")` and `:version #.+v+` are all
//!   simply "present". Whether the value is a legal ASDF version string is a
//!   different question and one this rule deliberately leaves alone.
//! - **A conditionally-supplied version counts as present.** `#-slow :version`
//!   reads as one atom, and `crate::support::plist_mentions` matches it as a
//!   substring for exactly that reason.
//! - **Nothing is inherited.** ASDF does not give a system a version because
//!   some other system in the file has one, and neither does this rule.
//! - **No fix.** What the version *should be* is a human decision, which is why
//!   `Fixability::ReportOnly`.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    defsystem_name, defsystem_options, for_each_evaluated_subview, is_defsystem, plist_mentions,
};

#[derive(Debug, Clone)]
pub struct AsdfSystemMissingVersionItem {
    /// The span of the whole `(defsystem …)` form: a `:version` could be
    /// written anywhere in its option plist, so no narrower span is the place
    /// the option is missing from.
    pub span: ByteSpan,
    /// The system's name, as ASDF's `coerce-name` would produce it.
    pub system: String,
}

impl Finding for AsdfSystemMissingVersionItem {
    fn kind(&self) -> &'static str {
        "asdf-system-missing-version"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("system={}", self.system)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("system", json!(self.system))]
    }

    /// States only what was observed. An earlier wording claimed no
    /// `(:version "x" …)` floor could be satisfied, which is **false** for at
    /// least one real system: `cffi.asd` omits `:version` on purpose and ships
    /// a `version-satisfies` method making every floor succeed. A rule may not
    /// assert a consequence the code can override.
    fn message(&self) -> String {
        format!(
            "system `{}` declares no :version, so asdf:component-version returns NIL for it",
            self.system
        )
    }
}

/// The option whose absence this rule is about.
const VERSION_OPTION: &str = ":version";

/// Name segments that mark a system as build-support rather than a released
/// artifact.
///
/// Nothing version-pins a test, example or benchmark system: it is not in
/// Quicklisp as a dependency, no `(:version …)` floor names it, and no release
/// script reads its version. Requiring one there is noise.
const SUPPORT_SEGMENTS: [&str; 12] = [
    "test",
    "tests",
    "testing",
    "example",
    "examples",
    "benchmark",
    "benchmarks",
    "bench",
    "demo",
    "demos",
    "bootstrap",
    "docs",
];

/// Whether `name` names a system this rule has no opinion about.
///
/// Two shapes, and the second exists because the first was not enough. A survey
/// of 104 primary systems across a local Quicklisp checkout produced 36
/// findings, **26 of which were test, example or benchmark systems** — and they
/// were missed because the ecosystem's dominant spelling for those is not the
/// ASDF secondary system this rule originally exempted:
///
/// - **Secondary systems**, `"app/tests"` — a `/` in the name. ASDF splits on
///   the first `/`: the part before it names the primary system that owns the
///   `.asd`, and the whole name is a secondary system defined in that same
///   file. These conventionally carry no `:version`.
/// - **Support systems in their own `.asd`**, `babel-tests`, `trivia.test`,
///   `cl-prolog2.swi.test`, `cffi-examples`, `named-readtables-test`. ASDF
///   calls each of these a *primary* system, so the `/` rule never saw them.
///   Recognized by segment instead: the name is split on `-`, `.` and `/`, and
///   any segment in [`SUPPORT_SEGMENTS`] exempts it.
///
/// Matching *any* segment rather than only the last is deliberate — it exempts
/// `dref-test-package-inferred` as well as `dref-test` — and so is the
/// direction of the resulting error. A shipped library that happens to contain
/// `test` in a segment (`should-test`) is exempted and goes unreported; that is
/// a missed finding, which this rule prefers to a wrong one.
#[must_use]
pub fn is_exempt_system_name(name: &str) -> bool {
    if name.contains('/') {
        return true;
    }
    name.split(['-', '.'])
        .any(|segment| SUPPORT_SEGMENTS.contains(&segment))
}

///
/// `checked_system_count` counts only the systems this rule has an opinion
/// about — see [`is_exempt_system_name`] on why support systems are excluded
/// from the denominator as well as from the findings.
pub fn examine_defsystem(
    view: &ExpressionView,
    checked_system_count: &mut usize,
    violations: &mut Vec<AsdfSystemMissingVersionItem>,
) {
    if !is_defsystem(view) {
        return;
    }
    // A `(defsystem)` with no name designator at all is a malformed form whose
    // subject this rule cannot identify; naming it in a finding is not possible
    // and guessing is not worth it.
    let Some(system) = defsystem_name(view) else {
        return;
    };
    if is_exempt_system_name(&system) {
        return;
    }
    *checked_system_count += 1;

    if plist_mentions(defsystem_options(view), VERSION_OPTION) {
        return;
    }
    violations.push(AsdfSystemMissingVersionItem {
        span: view.span,
        system,
    });
}

/// Collects every primary system with no `:version` in one file, with the
/// number of primary systems scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_asdf_system_missing_version_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AsdfSystemMissingVersionItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("checked_system_count", json!(0))],
        ));
    }

    let mut checked_system_count = 0;
    let mut violations = Vec::new();
    for_each_evaluated_subview(&tree.root_view(), |view| {
        examine_defsystem(view, &mut checked_system_count, &mut violations);
    });

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("checked_system_count", json!(checked_system_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AsdfSystemMissingVersionItem> {
        // `parse_with_dialect`, never the legacy `SyntaxTree::parse`: reader
        // syntax such as `#+sbcl` and `#.` is folded differently by the two,
        // and the CLI path uses this one.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_asdf_system_missing_version_report(Path::new("app.asd"), Dialect::CommonLisp, &tree)
            .expect("build asdf-system-missing-version report")
    }

    fn systems(input: &str) -> (u64, Vec<AsdfSystemMissingVersionItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "checked_system_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("checked_system_count in the summary");
        (count, report.findings)
    }

    // --- positive

    #[test]
    fn flags_a_primary_system_with_no_version() {
        let (count, violations) = systems(
            "(defsystem \"app\"\n  :description \"An app\"\n  :depends-on (\"alexandria\"))",
        );
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].system, "app");
    }

    #[test]
    fn flags_every_qualified_spelling_of_defsystem() {
        for head in [
            "defsystem",
            "asdf:defsystem",
            "asdf::defsystem",
            "asdf/parse-defsystem:defsystem",
        ] {
            let (_, violations) = systems(&format!("({head} \"app\")"));
            assert_eq!(violations.len(), 1, "not flagged with head `{head}`");
        }
    }

    #[test]
    fn a_symbol_name_designator_is_folded_the_way_asdf_folds_it() {
        let (_, violations) = systems("(defsystem #:MyApp)");
        assert_eq!(violations[0].system, "myapp");
    }

    // --- near-miss negatives

    #[test]
    fn does_not_flag_a_system_that_declares_a_version() {
        let (count, violations) = systems("(defsystem \"app\" :version \"1.0.2\" :serial t)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_version_supplied_by_a_reader_form() {
        let (_, violations) =
            systems("(defsystem \"app\" :version (:read-file-form \"version.sexp\"))");
        assert!(violations.is_empty());
    }

    /// FP-1, from the Quicklisp survey: 26 of 36 findings were test, example
    /// and benchmark systems spelled as *primary* systems in their own `.asd`,
    /// which the original `/`-only exemption never saw. These are the exact
    /// names that were wrongly flagged.
    #[test]
    fn does_not_flag_the_ecosystems_real_support_system_names() {
        for name in [
            "babel-tests",
            "cffi-tests",
            "cffi-examples",
            "alexandria-tests",
            "trivia.test",
            "trivia.benchmark",
            "named-readtables-test",
            "cl-prolog2.swi.test",
            "external-program-test",
            "global-vars-test",
            "trivial-features-tests",
            "lisp-namespace.test",
            "dref-test",
            "dref-test-package-inferred",
            "mgl-pax-bootstrap",
            "cl-colors-tests",
            "type-i.test",
        ] {
            let (count, violations) = systems(&format!("(defsystem \"{name}\")"));
            assert!(violations.is_empty(), "flagged support system `{name}`");
            assert_eq!(count, 0, "counted support system `{name}`");
        }
    }

    #[test]
    fn a_released_library_name_is_not_mistaken_for_a_support_system() {
        for name in ["babel", "cffi-grovel", "alexandria", "cl-ppcre-unicode"] {
            let (count, violations) = systems(&format!("(defsystem \"{name}\")"));
            assert_eq!(count, 1, "did not check released system `{name}`");
            assert_eq!(violations.len(), 1, "did not flag released `{name}`");
        }
    }

    /// FP-2: `cffi.asd` omits `:version` deliberately and defines a
    /// `version-satisfies` method so that every floor naming it succeeds. The
    /// message must therefore not claim anything about floors.
    #[test]
    fn the_message_claims_only_what_is_observable() {
        let (_, violations) = systems("(defsystem \"cffi\")");
        let message = violations[0].message();
        assert!(message.contains("declares no :version"));
        assert!(message.contains("component-version returns NIL"));
        assert!(
            !message.contains("floor"),
            "the message must not assert a consequence `version-satisfies` can override: {message}"
        );
    }

    /// FP-6: the dialect-aware reader folds `#-slow :version` into one atom, so
    /// an equality test on the keyword never sees it. A conditionally-supplied
    /// option must read as present, not absent.
    #[test]
    fn a_reader_conditional_attached_to_the_version_keyword_still_counts_as_present() {
        let (count, violations) = systems("(defsystem \"app\" #-slow :version #-slow \"1.0\")");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    /// The idiomatic two-system `.asd`: the primary carries the version, the
    /// test system does not and must not be asked to.
    #[test]
    fn does_not_flag_a_secondary_system_and_does_not_count_it() {
        let (count, violations) = systems(
            "(defsystem \"app\"\n\
             \x20 :version \"1.0.2\"\n\
             \x20 :in-order-to ((test-op (test-op \"app/tests\"))))\n\
             (defsystem \"app/tests\"\n\
             \x20 :depends-on (\"app\" \"fiveam\")\n\
             \x20 :perform (test-op (o c) (symbol-call :fiveam :run! :app)))\n",
        );
        assert_eq!(
            count, 1,
            "only the primary system belongs in the denominator"
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn a_dependency_version_floor_is_not_the_systems_own_version() {
        // `:version` here is nested inside a `:depends-on` entry. Reading it as
        // the system's own would silence a real finding.
        let (_, violations) =
            systems("(defsystem \"app\" :depends-on ((:version \"alexandria\" \"1.0\")))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_malformed_plist_that_still_mentions_version_is_left_alone() {
        // `#+sbcl` occupies a key slot, so pair alignment is unknowable. The
        // option is mentioned, so this stays silent — a missed finding rather
        // than a guessed one.
        let (_, violations) = systems("(defsystem \"app\" #+sbcl :serial t :version \"1.0\")");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_nameless_defsystem() {
        let (count, violations) = systems("(defsystem)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_form_that_merely_ends_in_defsystem() {
        let (count, violations) = systems("(mk:mk-defsystem \"app\")");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    // --- quote/quasiquote negatives (the five shapes)

    #[test]
    fn a_hard_quoted_defsystem_is_list_data_and_is_not_flagged() {
        let (count, violations) = systems("'(defsystem \"app\")");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_list_data_and_is_not_flagged() {
        let (count, violations) = systems("(quote (defsystem \"app\"))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_still_list_data() {
        let (count, violations) = systems("'(a ,(defsystem \"app\"))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_list_data() {
        let (count, violations) = systems("`(defsystem \"app\")");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn an_unquoted_defsystem_inside_a_backquote_is_code_and_is_flagged() {
        let (count, violations) = systems("`(a ,(defsystem \"app\"))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    // --- string-literal negative

    #[test]
    fn a_defsystem_inside_a_string_literal_is_one_atom_and_is_not_a_form() {
        let (count, violations) = systems("(format t \"(defsystem \\\"app\\\")\")");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    // --- envelope

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defsystem \"app\")", Dialect::Clojure).expect("parse");
        let report =
            build_asdf_system_missing_version_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("checked_system_count", json!(0))]);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_system_name() {
        let report = report("\n(defsystem \"app\"\n  :serial t)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "asdf-system-missing-version");
        assert_eq!(finding.json_fields(), vec![("system", json!("app"))]);
        assert_eq!(finding.text_columns(), vec!["system=app".to_owned()]);
        assert!(finding.message().contains("declares no :version"));
    }

    #[test]
    fn the_summary_counts_every_primary_system_not_only_the_flagged_ones() {
        let report = report(
            "(defsystem \"a\" :version \"1\")\n(defsystem \"b\")\n(defsystem \"c/tests\")\n",
        );
        assert_eq!(report.summary, vec![("checked_system_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
