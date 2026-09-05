//! An ASDF system that depends on itself.
//!
//! `(defsystem "app" :depends-on ("alexandria" "app"))` is a one-node
//! dependency cycle. ASDF `operate`s each dependency before the component that
//! names it, so a system naming itself is asked to be loaded before it can be
//! loaded; ASDF reports that as a circular dependency and the build stops. It is
//! a typo defect — a copied `:depends-on` line, a renamed system whose old name
//! stayed behind — and it is invisible to every cycle report in this repo, for a
//! reason worth stating here: `paredit_core_syntax::graph::string_edge_cycles`
//! **deliberately drops self-loops** before running Tarjan
//! (`if source_index != target_index`) and then keeps only components of size
//! `> 1`, because a self-loop cannot create a *multi-node* cycle. `inspect
//! system-cycles` is a thin wrapper over that, so the `("app", "app")` edge is
//! produced upstream and then discarded. This rule is the deliberate new policy
//! that a self-loop is a finding; it does not change the shared graph core,
//! which four reports depend on.
//!
//! # What this rule does *not* attempt
//!
//! - **A forward reference in the same `.asd` is not a defect.** ASDF loads a
//!   whole `.asd` file, registering every `defsystem` in it, *before* resolving
//!   any dependency, so declaration order inside one file carries no meaning.
//!   The most common two-system `.asd` in the ecosystem relies on this — the
//!   primary system's `:in-order-to` names a test system defined *below* it, as
//!   `alexandria.asd` does. Flagging that would be a false positive on correct,
//!   idiomatic code, so declaration order is not consulted at all.
//! - **Only the three dependency options are read**: `:depends-on`,
//!   `:defsystem-depends-on` and `:weakly-depends-on`. `:in-order-to`,
//!   `:perform` and friends name *operations* on systems, and
//!   `(test-op (test-op "self"))` is the normal way a system delegates its own
//!   test-op — a different question with a different answer.
//! - **`(:require …)` is skipped.** It names an implementation module
//!   (`:sb-posix`), not an ASDF system, and shares no namespace with one.
//! - **Nothing outside the form is consulted.** A dependency naming a system
//!   defined in another file is somebody else's; only the enclosing
//!   `defsystem`'s own name is compared against.
//! - **No fix.** Whether the entry should be deleted, or renamed to the system
//!   that was meant, is a human decision.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list};
use serde_json::{Value, json};

use crate::support::{
    asdf_name, defsystem_name, defsystem_options, for_each_evaluated_subview, is_defsystem,
    plist_value, symbol_name,
};

#[derive(Debug, Clone)]
pub struct AsdfSelfReferentialDependsOnItem {
    /// The span of the offending dependency *entry*, not of the whole
    /// `defsystem`: the entry is what would be deleted or renamed.
    pub span: ByteSpan,
    /// The system that depends on itself, as ASDF's `coerce-name` produces it.
    pub system: String,
    /// Which of the three dependency options carried it.
    pub option: String,
}

impl Finding for AsdfSelfReferentialDependsOnItem {
    fn kind(&self) -> &'static str {
        "asdf-self-referential-depends-on"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("system={}", self.system),
            format!("option={}", self.option),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("system", json!(self.system)),
            ("option", json!(self.option)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "system `{}` names itself in {}; ASDF must load it before it can load it, \
             which it reports as a circular dependency",
            self.system, self.option
        )
    }
}

/// The three options that declare a dependency on another *system*. ASDF's own
/// `parse-defsystem` reads exactly these three into a system's dependency
/// lists.
const DEPENDENCY_OPTIONS: [&str; 3] =
    [":depends-on", ":defsystem-depends-on", ":weakly-depends-on"];

/// The options this rule reads, for callers and tests that must not restate
/// them.
#[must_use]
pub const fn dependency_options() -> [&'static str; 3] {
    DEPENDENCY_OPTIONS
}

/// Whether `option` is one of them.
#[must_use]
pub fn is_dependency_option(option: &str) -> bool {
    DEPENDENCY_OPTIONS.contains(&option)
}

/// The system a dependency entry names, or `None` when it names none.
///
/// ASDF accepts four spellings, and they put the name in three different
/// places:
///
/// ```text
/// "alexandria"                     a bare designator
/// (:version "alexandria" "1.0")    name at 1
/// (:feature :sbcl "sb-posix")      name at 2, itself a dependency entry
/// (:require :sb-posix)             an implementation module, not a system
/// ```
///
/// `(:require …)` answers `None` on purpose: a `require`d module lives in the
/// implementation's namespace, not ASDF's, so it can never be the enclosing
/// system.
fn dependency_name(entry: &ExpressionView) -> Option<String> {
    if let Some(text) = atom_text(entry) {
        return asdf_name(text);
    }
    if !is_paren_list(entry) {
        return None;
    }
    let head = symbol_name(entry.children.first()?)?;
    match head.as_str() {
        ":version" => atom_text(entry.children.get(1)?).and_then(asdf_name),
        // A `:feature` wrapper's payload is itself a dependency entry, so the
        // nested `(:version …)` spelling keeps working inside it.
        ":feature" => dependency_name(entry.children.get(2)?),
        _ => None,
    }
}

/// The entries of one dependency option's value. ASDF wants a list but
/// tolerates a bare designator, and both spellings occur in the wild.
fn dependency_entries(value: &ExpressionView) -> &[ExpressionView] {
    if is_paren_list(value) {
        &value.children
    } else {
        std::slice::from_ref(value)
    }
}

///
/// `dependency_count` counts every entry read across the three options — the
/// denominator a rate of "dependencies that name their own system" is taken
/// against.
pub fn examine_defsystem(
    view: &ExpressionView,
    dependency_count: &mut usize,
    violations: &mut Vec<AsdfSelfReferentialDependsOnItem>,
) {
    if !is_defsystem(view) {
        return;
    }
    let Some(system) = defsystem_name(view) else {
        return;
    };
    let options = defsystem_options(view);

    for option in DEPENDENCY_OPTIONS {
        let Some(value) = plist_value(options, option) else {
            continue;
        };
        for entry in dependency_entries(value) {
            *dependency_count += 1;
            if dependency_name(entry).is_some_and(|name| name == system) {
                violations.push(AsdfSelfReferentialDependsOnItem {
                    span: entry.span,
                    system: system.clone(),
                    option: option.to_owned(),
                });
            }
        }
    }
}

/// Collects every self-referential dependency in one file, with the number of
/// dependency entries scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_asdf_self_referential_depends_on_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AsdfSelfReferentialDependsOnItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("dependency_count", json!(0))],
        ));
    }

    let mut dependency_count = 0;
    let mut violations = Vec::new();
    for_each_evaluated_subview(&tree.root_view(), |view| {
        examine_defsystem(view, &mut dependency_count, &mut violations);
    });

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("dependency_count", json!(dependency_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AsdfSelfReferentialDependsOnItem> {
        // `parse_with_dialect`, never the legacy `SyntaxTree::parse`.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_asdf_self_referential_depends_on_report(
            Path::new("app.asd"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build asdf-self-referential-depends-on report")
    }

    fn deps(input: &str) -> (u64, Vec<AsdfSelfReferentialDependsOnItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "dependency_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("dependency_count in the summary");
        (count, report.findings)
    }

    // --- positive

    #[test]
    fn flags_a_system_that_lists_itself() {
        let (count, violations) = deps("(defsystem \"app\" :depends-on (\"alexandria\" \"app\"))");
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].system, "app");
        assert_eq!(violations[0].option, ":depends-on");
    }

    #[test]
    fn flags_a_self_reference_in_each_of_the_three_dependency_options() {
        for option in dependency_options() {
            let (_, violations) = deps(&format!("(defsystem \"app\" {option} (\"app\"))"));
            assert_eq!(violations.len(), 1, "not flagged under `{option}`");
            assert_eq!(violations[0].option, option);
        }
    }

    #[test]
    fn flags_a_self_reference_through_the_version_spelling() {
        let (_, violations) = deps("(defsystem \"app\" :depends-on ((:version \"app\" \"1.0\")))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_a_self_reference_through_the_feature_spelling() {
        let (_, violations) = deps("(defsystem \"app\" :depends-on ((:feature :sbcl \"app\")))");
        assert_eq!(violations.len(), 1);
    }

    /// `:App` reads as the symbol `APP` and `coerce-name` downcases it, so this
    /// really is one system named twice.
    #[test]
    fn two_symbol_designators_naming_one_system_are_recognized() {
        let (_, violations) = deps("(defsystem :App :depends-on (#:APP))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].system, "app");
    }

    #[test]
    fn flags_a_bare_designator_where_asdf_wanted_a_list() {
        let (count, violations) = deps("(defsystem \"app\" :depends-on \"app\")");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    // --- near-miss negatives

    #[test]
    fn does_not_flag_a_dependency_on_a_different_system() {
        let (count, violations) =
            deps("(defsystem \"app\" :depends-on (\"alexandria\" \"bordeaux-threads\"))");
        assert_eq!(count, 2);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_secondary_system_is_a_different_system_from_its_primary() {
        let (_, violations) = deps("(defsystem \"app\" :depends-on (\"app/tests\"))");
        assert!(violations.is_empty());
    }

    /// A primary system whose `:depends-on` names another system defined
    /// *below* it in the same file. ASDF registers every `defsystem` in the
    /// file before resolving any of them, so this is legal and common.
    ///
    /// The reference goes through `:depends-on` deliberately. Routing it
    /// through `:in-order-to` would make the test pass whether or not the rule
    /// reads that option at all — the count assertion below is what proves
    /// both dependencies were actually read, and the empty findings then mean
    /// "read and correctly not flagged" rather than "never looked at". It is
    /// also the discriminating case for *which* name the rule compares
    /// against: a rule that compared each dependency to the set of systems the
    /// file defines, rather than to its own enclosing system, would flag
    /// `app -> app-core` here.
    #[test]
    fn does_not_flag_a_forward_reference_within_one_asd_file() {
        let (count, violations) = deps(
            "(defsystem \"app\"\n\
             \x20 :version \"1.4\"\n\
             \x20 :depends-on (\"app-core\"))\n\
             (defsystem \"app-core\"\n\
             \x20 :depends-on (\"alexandria\"))\n",
        );
        assert_eq!(count, 2, "both systems' dependencies should have been read");
        assert!(violations.is_empty());
    }

    /// `(test-op (test-op "self"))` is how a system delegates its own test-op.
    /// It is not a dependency option and must never be read as one.
    #[test]
    fn does_not_flag_an_in_order_to_naming_the_same_system() {
        let (count, violations) =
            deps("(defsystem \"app\" :in-order-to ((test-op (test-op \"app\"))))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_require_of_an_implementation_module() {
        let (count, violations) =
            deps("(defsystem \"sb-posix\" :depends-on ((:require :sb-posix)))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    /// A string designator is case-preserving under `coerce-name` while a
    /// symbol one is case-folding, so these are two different systems and
    /// claiming otherwise would be a false positive.
    #[test]
    fn two_string_designators_differing_only_in_case_are_not_one_system() {
        let (_, violations) = deps("(defsystem \"App\" :depends-on (\"app\"))");
        assert!(violations.is_empty());
    }

    #[test]
    fn a_desynchronized_plist_is_not_read_as_pairs_at_all() {
        // `#+sbcl` occupies a key slot; every following pair is off by one, so
        // the scan bails rather than reading a value as a key.
        let (count, violations) =
            deps("(defsystem \"app\" #+sbcl :serial t :depends-on (\"app\"))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_nameless_defsystem() {
        let (count, violations) = deps("(defsystem :depends-on (\"app\"))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    // --- quote/quasiquote negatives (the five shapes)

    #[test]
    fn a_hard_quoted_defsystem_is_list_data_and_is_not_flagged() {
        let (count, violations) = deps("'(defsystem \"app\" :depends-on (\"app\"))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_list_data_and_is_not_flagged() {
        let (count, violations) = deps("(quote (defsystem \"app\" :depends-on (\"app\")))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_still_list_data() {
        let (count, violations) = deps("'(a ,(defsystem \"app\" :depends-on (\"app\")))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_list_data() {
        let (count, violations) = deps("`(defsystem \"app\" :depends-on (\"app\"))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn an_unquoted_defsystem_inside_a_backquote_is_code_and_is_flagged() {
        let (count, violations) = deps("`(a ,(defsystem \"app\" :depends-on (\"app\")))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    // --- string-literal negative

    #[test]
    fn a_defsystem_inside_a_string_literal_is_one_atom_and_is_not_a_form() {
        let (count, violations) =
            deps("(format t \"(defsystem \\\"app\\\" :depends-on (\\\"app\\\"))\")");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    // --- envelope

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(defsystem \"app\" :depends-on (\"app\"))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_asdf_self_referential_depends_on_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("dependency_count", json!(0))]);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report("\n(defsystem \"app\"\n  :depends-on (\"app\"))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "asdf-self-referential-depends-on");
        assert_eq!(
            finding.json_fields(),
            vec![("system", json!("app")), ("option", json!(":depends-on"))]
        );
        assert!(finding.message().contains("names itself"));
    }

    #[test]
    fn the_option_names_are_the_ones_the_predicate_accepts() {
        for option in dependency_options() {
            assert!(is_dependency_option(option));
        }
        assert!(!is_dependency_option(":in-order-to"));
    }
}
