//! Each rule against the code it must flag and the code it must not.
//!
//! The golden corpus in `tests/fixtures/lint_golden/emacs-lisp.el` already
//! pins that every rule here fires end-to-end. What it cannot show is the
//! other half: that a *correct* file produces nothing. A rule that reported
//! unconditionally would pass the golden test and be worthless, so every case
//! below is paired.

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path;

/// The rules of this crate, in one catalogue, so a test run dispatches through
/// the real engine rather than calling `check` directly.
const CATALOG: [RuleEntry; 9] = [
    RuleEntry::new(
        &crate::missing_lexical_binding::rule::META,
        &crate::missing_lexical_binding::rule::RULE,
    ),
    RuleEntry::new(
        &crate::unreachable_lexical_binding::rule::META,
        &crate::unreachable_lexical_binding::rule::RULE,
    ),
    RuleEntry::new(
        &crate::autoload_cookie_without_form::rule::META,
        &crate::autoload_cookie_without_form::rule::RULE,
    ),
    RuleEntry::new(
        &crate::defcustom_missing_type::rule::META,
        &crate::defcustom_missing_type::rule::RULE,
    ),
    RuleEntry::new(
        &crate::defcustom_missing_group::rule::META,
        &crate::defcustom_missing_group::rule::RULE,
    ),
    RuleEntry::new(
        &crate::obsolete_cl_alias::rule::META,
        &crate::obsolete_cl_alias::rule::RULE,
    ),
    RuleEntry::new(
        &crate::quoted_lambda::rule::META,
        &crate::quoted_lambda::rule::RULE,
    ),
    RuleEntry::new(
        &crate::interactive_in_macro::rule::META,
        &crate::interactive_in_macro::rule::RULE,
    ),
    RuleEntry::new(
        &crate::condition_case_without_handler::rule::META,
        &crate::condition_case_without_handler::rule::RULE,
    ),
];

/// A first line that satisfies `elisp-missing-lexical-binding`, so a fixture
/// about some other rule does not also trip that one.
const LEXICAL: &str = ";;; f.el --- x -*- lexical-binding: t -*-\n";

fn outcomes_in(dialect: Dialect, source: &str) -> Vec<LintOutcome> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("fixture parses");
    let catalog = RuleCatalog::new(&CATALOG);
    let index = build_head_index(catalog);
    collect_lint_outcomes(
        catalog,
        &index,
        Path::new("f.el"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("the engine runs")
}

/// The rule names that fire on `body`, which carries a lexical header.
fn rules_for(body: &str) -> Vec<&'static str> {
    rules_for_file(&format!("{LEXICAL}{body}"))
}

/// The rule names that fire on `source` exactly as written.
fn rules_for_file(source: &str) -> Vec<&'static str> {
    outcomes_in(Dialect::EmacsLisp, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

/// The message of the single finding `source` produces.
fn sole_message(source: &str) -> String {
    let mut outcomes = outcomes_in(Dialect::EmacsLisp, source);
    assert_eq!(outcomes.len(), 1, "expected exactly one finding");
    outcomes.remove(0).into_parts().0.message
}

#[test]
fn a_file_without_a_lexical_binding_header_is_reported_once() {
    assert_eq!(
        rules_for_file("(defun f () nil)\n"),
        ["elisp-missing-lexical-binding"]
    );
}

#[test]
fn a_file_that_turns_lexical_binding_off_on_purpose_is_left_alone() {
    // Some files genuinely need dynamic binding, and one that says so has
    // made a decision rather than forgotten one.
    assert_eq!(
        rules_for_file(";;; f.el -*- lexical-binding: nil -*-\n(defun f () nil)\n"),
        [] as [&str; 0]
    );
}

#[test]
fn an_empty_file_is_not_reported() {
    assert_eq!(rules_for_file("\n  \n"), [] as [&str; 0]);
}

#[test]
fn a_lexical_binding_setting_below_the_first_line_is_reported_as_unreachable() {
    let source =
        ";;; f.el --- x\n(defun f () nil)\n\n;; Local Variables:\n;; lexical-binding: t\n;; End:\n";
    let rules = rules_for_file(source);

    // Both fire, and they are two different statements: the file is dynamic,
    // *and* the setting that was meant to prevent that is in the wrong place.
    assert!(rules.contains(&"elisp-missing-lexical-binding"));
    assert!(rules.contains(&"elisp-unreachable-lexical-binding"));
}

#[test]
fn a_shebang_defers_the_header_to_line_two_for_this_rule_too() {
    // The header Emacs reads is on line 2 here, so the rule must not report
    // the very setting that is working.
    let source = "#!/usr/bin/emacs --script\n                  ;;; -*- lexical-binding: t -*-\n                  (defun f () nil)\n";
    assert_eq!(rules_for_file(source), [] as [&str; 0]);
}

#[test]
fn prose_mentioning_lexical_binding_is_not_a_setting() {
    let source = ";;; f.el --- x -*- lexical-binding: t -*-\n\
                  ;; This file needs lexical-binding to be enabled.\n\
                  (defun f () nil)\n";
    assert_eq!(rules_for_file(source), [] as [&str; 0]);
}

#[test]
fn an_autoload_cookie_with_a_following_definition_is_accepted() {
    assert_eq!(
        rules_for(";;;###autoload\n(defun f () nil)\n"),
        [] as [&str; 0]
    );
}

#[test]
fn an_autoload_cookie_at_the_end_of_the_file_is_reported() {
    assert_eq!(
        rules_for("(defun f () nil)\n;;;###autoload\n"),
        ["elisp-autoload-cookie-without-form"]
    );
}

#[test]
fn an_autoload_cookie_nested_in_a_form_is_reported() {
    // `loaddefs` extracts top-level definitions only, so this produces no
    // autoload and says nothing about it.
    assert_eq!(
        rules_for("(progn\n  ;;;###autoload\n  (defun f () nil))\n"),
        ["elisp-autoload-cookie-without-form"]
    );
}

#[test]
fn a_cookie_carrying_its_own_form_needs_nothing_after_it() {
    // The rest of the line is what gets copied into the generated file.
    assert_eq!(
        rules_for("(defun f () nil)\n;;;###autoload (autoload 'f \"lib\")\n"),
        [] as [&str; 0]
    );
}

#[test]
fn a_defcustom_missing_type_and_group_is_reported_for_each() {
    assert_eq!(
        rules_for("(defcustom opt nil \"Doc.\")\n"),
        [
            "elisp-defcustom-missing-type",
            "elisp-defcustom-missing-group"
        ]
    );
}

#[test]
fn a_complete_defcustom_is_accepted() {
    assert_eq!(
        rules_for("(defcustom opt nil \"Doc.\"\n  :type 'boolean\n  :group 'mine)\n"),
        [] as [&str; 0]
    );
}

#[test]
fn a_keyword_appearing_as_a_value_does_not_satisfy_the_rule() {
    // `:type` here is the *value* of `:options`, not an option of its own.
    let rules = rules_for("(defcustom opt nil \"Doc.\"\n  :options :type\n  :group 'mine)\n");
    assert_eq!(rules, ["elisp-defcustom-missing-type"]);
}

#[test]
fn an_obsolete_cl_alias_is_reported_and_its_replacement_named() {
    let source = format!("{LEXICAL}(defun f () (loop for n from 1 to 3 collect n))\n");
    assert!(sole_message(&source).contains("cl-loop"));
}

#[test]
fn the_two_function_cell_aliases_say_that_the_replacement_differs() {
    // `cl-flet` is not a rename of `flet`: the old form rebound the function
    // cell for a dynamic extent, so a callee saw the replacement.
    for head in ["flet", "labels"] {
        let source = format!("{LEXICAL}(defun f () ({head} ((g () 1)) (g)))\n");
        assert!(sole_message(&source).contains("cl-letf"), "{head}");
    }
}

#[test]
fn a_cl_lib_prefixed_form_is_not_an_obsolete_alias() {
    assert_eq!(
        rules_for("(defun f () (cl-loop for n from 1 to 3 collect n))\n"),
        [] as [&str; 0]
    );
}

#[test]
fn a_quoted_lambda_is_reported_and_a_sharp_quoted_one_is_not() {
    assert_eq!(
        rules_for("(defun f () (mapcar '(lambda (n) n) xs))\n"),
        ["elisp-quoted-lambda"]
    );
    assert_eq!(
        rules_for("(defun f () (mapcar #'(lambda (n) n) xs))\n"),
        [] as [&str; 0]
    );
    assert_eq!(
        rules_for("(defun f () (mapcar (lambda (n) n) xs))\n"),
        [] as [&str; 0]
    );
}

#[test]
fn an_interactive_form_in_a_macro_is_reported_and_in_a_function_is_not() {
    assert_eq!(
        rules_for("(defmacro m (n) \"Doc.\" (interactive) n)\n"),
        ["elisp-interactive-in-macro"]
    );
    assert_eq!(
        rules_for("(defun f (n) \"Doc.\" (interactive) n)\n"),
        [] as [&str; 0]
    );
}

#[test]
fn an_interactive_call_deeper_in_a_macro_body_is_not_a_command_declaration() {
    // Past the docstring-and-`declare` header an `(interactive …)` is an
    // ordinary call, and calling `interactive` is legal.
    assert_eq!(
        rules_for("(defmacro m (n) \"Doc.\" (list n) (interactive))\n"),
        [] as [&str; 0]
    );
}

#[test]
fn a_declare_form_does_not_hide_the_interactive_after_it() {
    assert_eq!(
        rules_for("(defmacro m (n) \"Doc.\" (declare (indent 1)) (interactive) n)\n"),
        ["elisp-interactive-in-macro"]
    );
}

#[test]
fn a_condition_case_without_handlers_is_reported() {
    assert_eq!(
        rules_for("(defun f () (condition-case err (risky)))\n"),
        ["elisp-condition-case-without-handler"]
    );
}

#[test]
fn a_condition_case_with_a_handler_is_accepted() {
    assert_eq!(
        rules_for("(defun f () (condition-case err (risky) (error nil)))\n"),
        [] as [&str; 0]
    );
}

#[test]
fn every_rule_is_skipped_for_a_common_lisp_file() {
    // Each rule declares `Dialect::EmacsLisp` and nothing else, so a Common
    // Lisp run must not pay for them — and must not report on a `.lisp` file
    // that happens to contain a `loop` or a quoted lambda, both of which are
    // ordinary Common Lisp.
    let source = "(defun f () (loop for n from 1 to 3 collect n))\n\
                  (defun g () (mapcar '(lambda (n) n) xs))\n";
    assert_eq!(
        outcomes_in(Dialect::CommonLisp, source).len(),
        0,
        "Common Lisp must not see the Emacs Lisp rules"
    );
}

#[test]
fn an_uppercase_head_is_not_the_special_form_it_spells() {
    // Emacs Lisp reads symbols case-sensitively, so `LOOP` is a name a
    // package may define. The engine's head index folds case, which is why
    // every rule re-resolves the exact text.
    assert_eq!(rules_for("(defun f () (LOOP 1))\n"), [] as [&str; 0]);
}
