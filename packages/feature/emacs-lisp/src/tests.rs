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
fn every_obsolete_alias_is_still_reported_in_its_real_calling_shape() {
    // The negative control for the two guards below: narrowing the rule must
    // not have silenced any of the fourteen names in the form it is actually
    // written in.
    for body in [
        "(defun f () (block done (return-from done 1)))",
        "(defun f (x) (case x (1 'a) (t 'b)))",
        "(defun f (x) (ecase x (1 'a)))",
        "(defun f () (do ((i 0 (1+ i))) ((= i 3) i)))",
        "(defun f () (do* ((i 0 (1+ i))) ((= i 3) i)))",
        "(defun f () (loop for n from 1 to 3 collect n))",
        "(defun f () (flet ((g () 1)) (g)))",
        "(defun f () (labels ((g () 1)) (g)))",
        "(defun f () (macrolet ((g () 1)) (g)))",
        "(defun f () (symbol-macrolet ((g 1)) g))",
        "(defun f (x) (letf (((car x) 1)) x))",
        "(defun f (x) (letf* (((car x) 1)) x))",
        "(defun f (x) (destructuring-bind (a b) x (list a b)))",
        "(defun f (x) (multiple-value-bind (a b) x (list a b)))",
    ] {
        assert_eq!(
            rules_for(&format!("{body}\n")),
            ["elisp-obsolete-cl-alias"],
            "missed {body}"
        );
    }
}

#[test]
fn a_binding_pair_or_lambda_list_named_after_an_alias_is_not_a_call() {
    // Every one of these is real GNU Emacs 31 code that the rule reported.
    // None is evaluated as a call, so none can fail to load.
    for body in [
        // `help.el`, `markdown-ts-mode.el`: a `dolist` variable named `block`.
        "(defun f (blocks) (dolist (block blocks) block))",
        // `pcase.el`: a `dolist` variable named `case`.
        "(defun f (cases) (dolist (case cases) case))",
        // `sort.el`: a `let` binding named `do`.
        "(defun f () (let (ll (do t)) (while do (setq do nil)) ll))",
        // `xsd-regexp.el`: a `let` binding named `block`.
        "(defun f (name) (let ((block (intern name))) block))",
        // `mail-utils.el`: a `defun` parameter named `labels`.
        "(defun mail-comma-list-regexp (labels) labels)",
        // `pcase.el`: a `lambda` parameter named `case`.
        "(defun f (cases) (mapcar (lambda (case) (car case)) cases))",
        // `rmailkwd.el`: an arglist in a `declare-function`.
        "(declare-function mail-comma-list-regexp \"mail-utils\" (labels))",
    ] {
        assert_eq!(rules_for(&format!("{body}\n")), [] as [&str; 0], "{body}");
    }
}

#[test]
fn a_named_let_recursion_named_after_an_alias_is_a_call_to_itself() {
    // `byte-opt.el`, `bytecomp.el`, `package.el` and `oclosure.el` all write
    // `(named-let loop …)` and then call `loop` with two arguments — which the
    // arity guard cannot tell from `cl-loop`. Only the binding table can.
    let body = "(defun f (args acc) \
                (named-let loop ((args args) (acc acc)) \
                (if args (loop (cdr args) acc) acc)))";
    assert_eq!(rules_for(&format!("{body}\n")), [] as [&str; 0]);
}

#[test]
fn a_first_argument_of_the_wrong_shape_is_not_the_macro() {
    for body in [
        // `do` and its relatives open with a *binding list*. `sort.el` writes
        // `(let (ll (do t)) …)`, and `doctor.el` types `(do you know …)` at
        // the user; neither first argument is one.
        "(defun f () (do you know about this))",
        "(defun f () (labels are not bindings))",
        "(defun f () (destructuring-bind a b c))",
        // `block` opens with a *name*, which is a symbol. `markdown-ts-mode.el`
        // writes this exact `cond` clause, where `block` is a `let*` variable
        // and the clause is its value, not a call.
        "(defun f (block moved) \
         (cond ((block (goto-char (cdr block)) (setq moved (1+ moved))))))",
    ] {
        assert_eq!(rules_for(&format!("{body}\n")), [] as [&str; 0], "{body}");
    }
}

#[test]
fn a_quoted_list_headed_by_an_alias_is_data() {
    // `cl-macs.el` searches `'(do doing)` with `memq`; `doctor.el` types
    // `'(do you know Stallman \?)` at the user. Neither is evaluated, so
    // neither can fail to load. The third has the exact shape of a real
    // `case` call and is stopped only by the quote.
    for body in [
        "(defun f (word) (memq word '(do doing)))",
        "(defun f () (doctor-type '(do you know Stallman)))",
        "(defun f (form) (equal form '(case x (1 a))))",
        "(defun f (form) (equal form '(block done (return-from done 1))))",
    ] {
        assert_eq!(rules_for(&format!("{body}\n")), [] as [&str; 0], "{body}");
    }
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
fn a_quoted_symbol_list_starting_with_lambda_is_not_a_quoted_lambda() {
    // GNU Emacs writes all four of these against lists of *symbol names*:
    // `bind-key.el` and `byte-opt.el` search them with `memq`, and dropping
    // the quote would rewrite the membership test into a call. What tells
    // them apart from a lambda expression is the lambda list.
    for body in [
        "(defun f (e) (memq (car e) '(lambda function)))\n",
        "(defun f (e) (memq (car e) '(lambda)))\n",
        "(defun f (e) (memq (car e) '(lambda macro)))\n",
        "(defun f (e) (memq (car e) '(lambda internal-make-closure length cons)))\n",
    ] {
        assert_eq!(rules_for(body), [] as [&str; 0], "reported on {body}");
    }
}

#[test]
fn a_lambda_list_is_what_makes_a_quoted_lambda_one() {
    // The negative control for the check above: each of these *does* have a
    // lambda list, so narrowing the rule must not have silenced it. `nil` is
    // the argument-less spelling and is a lambda list too.
    for body in [
        "(defun f () (mapcar '(lambda (n) n) xs))\n",
        "(defun f () (mapcar '(lambda () 1) xs))\n",
        "(defun f () (mapcar '(lambda nil 1) xs))\n",
        "(defun f () (mapcar '(lambda (a &optional b) (list a b)) xs))\n",
    ] {
        assert_eq!(rules_for(body), ["elisp-quoted-lambda"], "missed {body}");
    }
}

#[test]
fn a_quote_that_is_not_the_only_reader_prefix_is_not_reported() {
    // `',(lambda …)` inside a backquote is `menu-bar.el`'s idiom for putting a
    // real closure into a generated form: the unquote evaluates the lambda, so
    // the quote applies to the closure. A second quote, or a `#'`, makes the
    // form data by construction.
    for body in [
        "(defun f () `(funcall ',(lambda () t)))\n",
        "(defun f () (equal x ''(lambda () t)))\n",
        "(defun f () (equal x '#'(lambda () t)))\n",
    ] {
        assert_eq!(rules_for(body), [] as [&str; 0], "reported on {body}");
    }
}

#[test]
fn a_quoted_lambda_inside_a_backquote_is_still_reported() {
    // The negative control for the prefix check: a lone quote is still a lone
    // quote inside a template, and this is the real defect the rule exists
    // for -- the generated code will contain a list, not a closure.
    assert_eq!(
        rules_for("(defmacro m () `(mapcar '(lambda (n) n) xs))\n"),
        ["elisp-quoted-lambda"]
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
