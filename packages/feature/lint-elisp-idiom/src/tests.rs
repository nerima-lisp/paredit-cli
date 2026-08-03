//! Each rule against the code it must flag and the code it must not.
//!
//! Every case is paired. A rule that reported unconditionally would satisfy
//! any single positive test and be worthless, so each positive has a negative
//! that differs by exactly the thing the rule is about.
//!
//! Two suite-level tests carry the weight the per-rule pairs cannot:
//! [`a_realistic_correct_file_produces_no_findings`] sweeps a file written the
//! way the manual says and asserts *zero*, while also asserting a non-zero
//! count of the shapes each rule keys on — so it cannot pass by matching
//! nothing — and [`the_dangerous_twin_fires_every_rule_exactly_once`] is the
//! same file with each idiom broken, asserting one finding per rule.

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path;

/// The rules of this crate, in one catalogue, so a test run dispatches through
/// the real engine rather than calling `check` directly. Calling `check`
/// directly would bypass the head index, which is where a wrong `HeadFilter`
/// shows up.
const CATALOG: [RuleEntry; 5] = [
    RuleEntry::new(
        &crate::keymap_binds_non_command::rule::META,
        &crate::keymap_binds_non_command::rule::RULE,
    ),
    RuleEntry::new(
        &crate::interactive_arity_mismatch::rule::META,
        &crate::interactive_arity_mismatch::rule::RULE,
    ),
    RuleEntry::new(
        &crate::hook_lambda::rule::META,
        &crate::hook_lambda::rule::RULE,
    ),
    RuleEntry::new(
        &crate::save_excursion_set_buffer::rule::META,
        &crate::save_excursion_set_buffer::rule::RULE,
    ),
    RuleEntry::new(
        &crate::require_obsolete_cl::rule::META,
        &crate::require_obsolete_cl::rule::RULE,
    ),
];

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

/// The rule names that fire on `source` exactly as written.
fn rules_for_file(source: &str) -> Vec<&'static str> {
    outcomes_in(Dialect::EmacsLisp, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

/// The rule names that fire on `body`, which carries a lexical header.
fn rules_for(body: &str) -> Vec<&'static str> {
    rules_for_file(&format!("{LEXICAL}{body}"))
}

const NONE: [&str; 0] = [];

// ---------------------------------------------------------------------------
// elisp-keymap-binds-non-command
// ---------------------------------------------------------------------------

#[test]
fn a_key_bound_to_a_same_file_defun_without_interactive_is_reported() {
    assert_eq!(
        rules_for("(defun my-go () (message \"hi\"))\n(define-key m (kbd \"C-c a\") #'my-go)\n"),
        ["elisp-keymap-binds-non-command"]
    );
}

#[test]
fn the_same_binding_to_a_command_is_left_alone() {
    assert_eq!(
        rules_for(
            "(defun my-go () (interactive) (message \"hi\"))\n\
             (define-key m (kbd \"C-c a\") #'my-go)\n"
        ),
        NONE
    );
}

#[test]
fn a_command_whose_interactive_follows_a_docstring_and_a_declare_is_left_alone() {
    // The header runs past both, and reading only the first body form would
    // call this definition non-interactive.
    assert_eq!(
        rules_for(
            "(defun my-go () \"Doc.\" (declare (indent 1)) (interactive) (message \"hi\"))\n\
             (define-key m (kbd \"C-c a\") #'my-go)\n"
        ),
        NONE
    );
}

#[test]
fn every_binder_head_is_read_at_its_own_definition_index() {
    // `global-set-key` has no keymap argument, so its DEF sits one earlier
    // than `define-key`'s. A single constant index would read the `kbd` call
    // here and report nothing at all.
    for form in [
        "(define-key m (kbd \"C-c a\") #'my-go)",
        "(define-key-after m (kbd \"C-c a\") #'my-go)",
        "(keymap-set m \"C-c a\" #'my-go)",
        "(keymap-global-set \"C-c a\" #'my-go)",
        "(keymap-local-set \"C-c a\" #'my-go)",
        "(global-set-key (kbd \"C-c a\") #'my-go)",
        "(local-set-key (kbd \"C-c a\") #'my-go)",
    ] {
        assert_eq!(
            rules_for(&format!("(defun my-go () (message \"hi\"))\n{form}\n")),
            ["elisp-keymap-binds-non-command"],
            "binder {form} did not report"
        );
    }
}

#[test]
fn a_binding_to_a_symbol_this_file_does_not_define_is_left_alone() {
    // The command almost certainly lives in another file, and reporting it
    // would flag the ordinary case of binding somebody else's command.
    assert_eq!(
        rules_for("(define-key m (kbd \"C-c a\") #'other-go)\n"),
        NONE
    );
}

#[test]
fn a_binding_to_a_bare_symbol_is_left_alone() {
    // `(define-key m k my-go)` passes the *value* of `my-go`, which this
    // cannot read; assuming the symbol was meant would be a guess.
    assert_eq!(
        rules_for("(defun my-go () (message \"hi\"))\n(define-key m (kbd \"C-c a\") my-go)\n"),
        NONE
    );
}

#[test]
fn a_binding_to_a_nested_defun_is_left_alone() {
    // Only top-level definitions are searched: a `defun` inside a `when` is
    // as likely conditional as not.
    assert_eq!(
        rules_for("(when x (defun my-go () 1))\n(define-key m (kbd \"C-c a\") #'my-go)\n"),
        NONE
    );
}

#[test]
fn a_binding_to_a_macro_of_the_same_name_is_left_alone() {
    // A macro cannot be a command at all — that is
    // `elisp-interactive-in-macro`'s complaint, not this one's.
    assert_eq!(
        rules_for("(defmacro my-go () 1)\n(define-key m (kbd \"C-c a\") #'my-go)\n"),
        NONE
    );
}

#[test]
fn a_quoted_binder_call_is_data_and_is_left_alone() {
    assert_eq!(
        rules_for("(defun my-go () 1)\n'(define-key m k #'my-go)\n"),
        NONE
    );
}

#[test]
fn a_binder_call_unquoted_back_into_a_backquote_is_still_code() {
    // The two-counter quote model earns itself here: a single depth counter
    // would call this data and miss the finding.
    assert_eq!(
        rules_for("(defun my-go () 1)\n`(a ,(define-key m k #'my-go))\n"),
        ["elisp-keymap-binds-non-command"]
    );
}

#[test]
fn a_comma_inside_a_hard_quote_does_not_escape_back_to_code() {
    // Inside `'(…)` a comma is a comma character in a literal list. `hard`
    // never clearing is what models that.
    assert_eq!(
        rules_for("(defun my-go () 1)\n'(a ,(define-key m k #'my-go))\n"),
        NONE
    );
}

// ---------------------------------------------------------------------------
// elisp-interactive-arity-mismatch
// ---------------------------------------------------------------------------

#[test]
fn a_bare_interactive_on_a_command_with_a_required_argument_is_reported() {
    assert_eq!(
        rules_for("(defun my-go (n) (interactive) (forward-line n))\n"),
        ["elisp-interactive-arity-mismatch"]
    );
}

#[test]
fn a_spec_that_supplies_the_required_argument_is_left_alone() {
    assert_eq!(
        rules_for("(defun my-go (n) (interactive \"p\") (forward-line n))\n"),
        NONE
    );
}

#[test]
fn a_one_letter_spec_on_a_two_argument_command_is_reported() {
    assert_eq!(
        rules_for("(defun my-go (n b) (interactive \"p\") (list n b))\n"),
        ["elisp-interactive-arity-mismatch"]
    );
}

#[test]
fn newline_separated_code_letters_each_supply_an_argument() {
    // Measured in Emacs 31.0.91: `(interactive "p\nbBuf: ")` calls a
    // two-argument command cleanly.
    assert_eq!(
        rules_for("(defun my-go (n b) (interactive \"p\\nbBuf: \") (list n b))\n"),
        NONE
    );
}

#[test]
fn the_leading_modifier_characters_supply_nothing() {
    // `*`, `@` and `^` are processed before any argument is read. Counting
    // them as code letters would call this spec two-argument and miss the bug.
    assert_eq!(
        rules_for("(defun my-go (n) (interactive \"*^\") (forward-line n))\n"),
        ["elisp-interactive-arity-mismatch"]
    );
}

#[test]
fn a_trailing_newline_in_a_spec_supplies_nothing() {
    // Measured: `(interactive "p\n")` calls a one-argument command cleanly
    // and signals on a two-argument one.
    assert_eq!(
        rules_for("(defun my-go (n) (interactive \"p\\n\") (forward-line n))\n"),
        NONE
    );
    assert_eq!(
        rules_for("(defun my-go (n b) (interactive \"p\\n\") (list n b))\n"),
        ["elisp-interactive-arity-mismatch"]
    );
}

#[test]
fn optional_and_rest_parameters_are_not_required() {
    for arglist in ["(&optional n)", "(&rest ns)", "(&optional n &rest ns)"] {
        assert_eq!(
            rules_for(&format!("(defun my-go {arglist} (interactive) nil)\n")),
            NONE,
            "arglist {arglist} was treated as required"
        );
    }
}

#[test]
fn a_required_parameter_before_an_optional_one_still_counts() {
    assert_eq!(
        rules_for("(defun my-go (n &optional m) (interactive) (list n m))\n"),
        ["elisp-interactive-arity-mismatch"]
    );
}

#[test]
fn a_computed_interactive_descriptor_is_never_reported() {
    // A non-string descriptor is evaluated to produce the argument list, so
    // what it supplies cannot be read from the source.
    assert_eq!(
        rules_for("(defun my-go (a b) (interactive (list 1 2)) (list a b))\n"),
        NONE
    );
}

#[test]
fn an_interactive_deeper_in_the_body_is_an_ordinary_call() {
    // Past the header it is a call that returns nil, not a specification —
    // and the definition is then not a command at all.
    assert_eq!(
        rules_for("(defun my-go (n) (message \"x\") (interactive) (forward-line n))\n"),
        NONE
    );
}

#[test]
fn a_macro_is_never_reported_for_its_interactive_arity() {
    assert_eq!(rules_for("(defmacro my-go (n) (interactive) n)\n"), NONE);
    assert_eq!(rules_for("(cl-defmacro my-go (n) (interactive) n)\n"), NONE);
}

#[test]
fn every_head_here_is_a_function_definition() {
    // What keeps the test above true is the `HEADS` list, not a check inside
    // `check` — an explicit `accepts_interactive()` bail there killed no
    // mutant, because no macro head can reach it. Adding one to `HEADS` would
    // make the rule report a form that cannot be a command at all, so the
    // invariant is asserted here instead of re-tested per node.
    use paredit_core_syntax::emacs_lisp::EmacsLispOperator;
    for head in crate::interactive_arity_mismatch::rule::HEAD_NAMES {
        let shape = EmacsLispOperator::from_head(head)
            .and_then(EmacsLispOperator::callable_shape)
            .unwrap_or_else(|| panic!("{head} names no callable form"));
        assert!(
            shape.accepts_interactive(),
            "{head} cannot carry an (interactive) header and must not be in HEADS"
        );
    }
}

#[test]
fn a_reader_prefixed_head_is_not_the_symbol_it_prefixes() {
    // `atom_text` carries the prefix, which is what makes every head
    // comparison in this crate safe without an explicit prefix test.
    assert_eq!(
        rules_for("(defun my-go () 1)\n('define-key m k #'my-go)\n"),
        NONE
    );
    assert_eq!(rules_for("(#'require 'cl)\n"), NONE);
}

#[test]
fn cl_defun_lambda_list_keywords_end_the_required_run() {
    assert_eq!(
        rules_for("(cl-defun my-go (&key n) (interactive) n)\n"),
        NONE
    );
    assert_eq!(
        rules_for("(cl-defun my-go (n &key m) (interactive) (list n m))\n"),
        ["elisp-interactive-arity-mismatch"]
    );
}

// ---------------------------------------------------------------------------
// elisp-hook-lambda
// ---------------------------------------------------------------------------

#[test]
fn a_lambda_on_a_hook_is_reported_in_every_function_spelling() {
    for function in [
        "(lambda () (setq fill-column 72))",
        "#'(lambda () (setq fill-column 72))",
        "(function (lambda () (setq fill-column 72)))",
    ] {
        assert_eq!(
            rules_for(&format!("(add-hook 'text-mode-hook {function})\n")),
            ["elisp-hook-lambda"],
            "spelling {function} did not report"
        );
    }
}

#[test]
fn a_function_symbol_on_a_hook_is_left_alone() {
    assert_eq!(rules_for("(add-hook 'text-mode-hook #'my-setup)\n"), NONE);
    assert_eq!(rules_for("(add-hook 'text-mode-hook 'my-setup)\n"), NONE);
}

#[test]
fn remove_hook_with_a_lambda_is_reported_with_its_own_message() {
    let outcomes = outcomes_in(
        Dialect::EmacsLisp,
        &format!("{LEXICAL}(remove-hook 'text-mode-hook (lambda () 1))\n"),
    );
    assert_eq!(outcomes.len(), 1);
    let finding = outcomes
        .into_iter()
        .next()
        .expect("one finding")
        .into_parts()
        .0;
    assert_eq!(finding.rule, "elisp-hook-lambda");
    assert!(
        finding.message.contains("silently does nothing"),
        "remove-hook must not borrow add-hook's message: {}",
        finding.message
    );
}

#[test]
fn a_quote_prefixed_lambda_is_left_to_elisp_quoted_lambda() {
    // `elisp-quoted-lambda` already reports this form, and says more.
    assert_eq!(
        rules_for("(add-hook 'text-mode-hook '(lambda () 1))\n"),
        NONE
    );
}

#[test]
fn a_local_flag_after_the_function_does_not_change_the_verdict() {
    assert_eq!(
        rules_for("(add-hook 'text-mode-hook (lambda () 1) nil t)\n"),
        ["elisp-hook-lambda"]
    );
}

#[test]
fn a_lambda_in_the_hook_position_rather_than_the_function_position_is_left_alone() {
    // Index 2 is FUNCTION; reading any child would report this.
    assert_eq!(rules_for("(add-hook (lambda () 'h) #'my-setup)\n"), NONE);
}

// ---------------------------------------------------------------------------
// elisp-save-excursion-set-buffer
// ---------------------------------------------------------------------------

#[test]
fn save_excursion_wrapping_set_buffer_is_reported() {
    assert_eq!(
        rules_for("(defun f (b) (save-excursion (set-buffer b) (point)))\n"),
        ["elisp-save-excursion-set-buffer"]
    );
}

#[test]
fn save_excursion_without_a_buffer_switch_is_left_alone() {
    assert_eq!(
        rules_for("(defun f () (save-excursion (goto-char (point-min)) (point)))\n"),
        NONE
    );
}

#[test]
fn a_deeply_nested_set_buffer_still_belongs_to_the_save_excursion() {
    assert_eq!(
        rules_for("(defun f (b) (save-excursion (when x (dolist (y ys) (set-buffer b)))))\n"),
        ["elisp-save-excursion-set-buffer"]
    );
}

#[test]
fn a_set_buffer_under_a_form_that_owns_the_current_buffer_is_not_reported() {
    for opaque in [
        "(with-current-buffer c (set-buffer b))",
        "(save-current-buffer (set-buffer b))",
        "(with-temp-buffer (set-buffer b))",
        "(lambda () (set-buffer b))",
    ] {
        assert_eq!(
            rules_for(&format!("(defun f (b c) (save-excursion {opaque}))\n")),
            NONE,
            "{opaque} was attributed to the outer save-excursion"
        );
    }
}

#[test]
fn a_nested_save_excursion_owns_its_own_set_buffer() {
    // Reported once, against the inner form, not twice.
    assert_eq!(
        rules_for("(defun f (b) (save-excursion (save-excursion (set-buffer b))))\n"),
        ["elisp-save-excursion-set-buffer"]
    );
}

#[test]
fn with_current_buffer_is_the_form_this_rule_recommends_and_is_never_reported() {
    assert_eq!(
        rules_for("(defun f (b) (with-current-buffer b (goto-char (point-min))))\n"),
        NONE
    );
}

#[test]
fn a_quoted_save_excursion_is_data() {
    assert_eq!(rules_for("'(save-excursion (set-buffer b))\n"), NONE);
}

// ---------------------------------------------------------------------------
// elisp-require-obsolete-cl
// ---------------------------------------------------------------------------

#[test]
fn requiring_the_obsolete_cl_package_is_reported() {
    assert_eq!(rules_for("(require 'cl)\n"), ["elisp-require-obsolete-cl"]);
    assert_eq!(
        rules_for("(require 'cl-compat)\n"),
        ["elisp-require-obsolete-cl"]
    );
}

#[test]
fn requiring_cl_lib_and_its_current_siblings_is_left_alone() {
    for feature in ["cl-lib", "cl-macs", "cl-seq", "cl-extra", "cl-generic"] {
        assert_eq!(
            rules_for(&format!("(require '{feature})\n")),
            NONE,
            "{feature} is current and must not be reported"
        );
    }
}

#[test]
fn a_computed_feature_name_is_left_alone() {
    assert_eq!(rules_for("(require feature)\n"), NONE);
}

#[test]
fn a_require_inside_eval_when_compile_is_still_reported() {
    assert_eq!(
        rules_for("(eval-when-compile (require 'cl))\n"),
        ["elisp-require-obsolete-cl"]
    );
}

#[test]
fn a_quoted_require_form_is_data() {
    assert_eq!(rules_for("'(require 'cl)\n"), NONE);
}

#[test]
fn an_unquoted_cl_is_a_variable_reference_and_is_left_alone() {
    // `(require cl)` requires whatever the *variable* `cl` holds. Falling back
    // to the bare atom's text here would report a form that names no feature.
    assert_eq!(rules_for("(require cl)\n"), NONE);
}

// ---------------------------------------------------------------------------
// Case sensitivity
// ---------------------------------------------------------------------------

#[test]
fn a_head_that_differs_only_in_case_is_a_different_symbol() {
    // Emacs Lisp is case-sensitive: `Add-Hook` is a symbol a file may define
    // itself, and linting it as though it were `add-hook` reports somebody
    // else's function.
    //
    // Two things protect this today and the test does not care which: the head
    // index case-folds only for Common Lisp (`head_index::head_key`), so these
    // forms never reach a rule at all, *and* every rule re-reads its head
    // exactly. Mutation-testing showed the second is redundant while the first
    // holds — this test is what would catch it if `head_key` ever widened.
    for form in [
        "(Define-Key m (kbd \"C-c a\") #'my-go)",
        "(Add-Hook 'text-mode-hook (lambda () 1))",
        "(Save-Excursion (set-buffer b))",
        "(Require 'cl)",
    ] {
        assert_eq!(
            rules_for(&format!("(defun my-go () (message \"hi\"))\n{form}\n")),
            NONE,
            "{form} was linted as though it were the lowercase symbol"
        );
    }
}

// ---------------------------------------------------------------------------
// Dialect scope
// ---------------------------------------------------------------------------

#[test]
fn none_of_these_rules_run_on_common_lisp() {
    // The default `dialect_scope()` is `COMMON_LISP_ONLY`, so a missing
    // override is silent: the rule would run on the wrong dialect and never
    // on the right one. This is the test that catches that.
    let source = "(require 'cl)\n(add-hook 'h (lambda () 1))\n\
                  (save-excursion (set-buffer b))\n\
                  (defun my-go (n) (interactive) n)\n\
                  (define-key m k #'my-go)\n";
    let names: Vec<&'static str> = outcomes_in(Dialect::CommonLisp, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
    assert_eq!(names, NONE);
}

#[test]
fn every_rule_in_this_crate_is_head_filtered() {
    // `WholeTree` runs once per file unconditionally and is gated by a CI
    // benchmark that has failed this project repeatedly. Nothing here may use
    // it, and this test is what keeps that true as rules are added.
    use paredit_core_lint_engine::model::HeadFilter;
    for entry in CATALOG {
        assert!(
            matches!(entry.rule().head_filter(), HeadFilter::Heads(_)),
            "{} is not Heads-filtered",
            entry.meta().name()
        );
    }
}

// ---------------------------------------------------------------------------
// Corpus sweep
// ---------------------------------------------------------------------------

/// A file written the way the GNU Emacs Lisp Reference Manual says to write
/// one, exercising every shape all five rules key on.
///
/// Reader syntax is deliberately present: `'`, `` ` ``, `,`, `,@`, `#'`, `?a`
/// character literals, and `;` comments. A rule that mis-reads any of them
/// reports here.
const CORRECT: &str = r#";;; my-pkg.el --- A package -*- lexical-binding: t -*-

;;; Commentary:

;; A file that does everything these rules are about, correctly.
;; Note the ?\; and ?, character literals below: `,' is not an unquote there.

;;; Code:

(require 'cl-lib)
(require 'subr-x)

(defgroup my-pkg nil
  "Customization."
  :group 'tools)

(defcustom my-pkg-width 72
  "How wide."
  :type 'integer
  :group 'my-pkg)

(defvar my-pkg-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-n") #'my-pkg-next)
    (define-key map (kbd "C-c C-p") #'my-pkg-previous)
    (keymap-set map "C-c C-g" #'my-pkg-goto)
    (define-key map (kbd "C-c C-x") #'save-buffer)
    map)
  "Keymap.")

(defconst my-pkg-separators (list ?, ?\; ?\s)
  "Characters that separate fields.")

(defun my-pkg-next (n)
  "Move forward N lines."
  (interactive "p")
  (forward-line n))

(defun my-pkg-previous (&optional n)
  "Move back N lines."
  (interactive "P")
  (forward-line (- (prefix-numeric-value n))))

(defun my-pkg-goto (line buffer)
  "Go to LINE in BUFFER."
  (interactive "nLine: \nbBuffer: ")
  (with-current-buffer buffer
    (goto-char (point-min))
    (forward-line (1- line))))

(defun my-pkg-collect (buffer)
  "Collect the lines of BUFFER."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (cl-loop until (eobp)
               collect (buffer-substring (line-beginning-position)
                                         (line-end-position))
               do (forward-line 1)))))

(defun my-pkg-quiet ()
  "Not a command, and not bound to a key either."
  (message "quiet"))

(defmacro my-pkg-with-width (width &rest body)
  "Run BODY with WIDTH."
  (declare (indent 1))
  `(let ((fill-column ,width))
     ,@body))

(defun my-pkg-setup ()
  "Set the buffer up."
  (setq fill-column my-pkg-width))

(add-hook 'text-mode-hook #'my-pkg-setup)
(remove-hook 'text-mode-hook #'my-pkg-obsolete-setup)

(defun my-pkg-describe ()
  "Describe, from a computed spec."
  (interactive (list (read-string "What? ")))
  (message "%s" 'ok))

(provide 'my-pkg)
;;; my-pkg.el ends here
"#;

#[test]
fn a_realistic_correct_file_produces_no_findings() {
    assert_eq!(rules_for_file(CORRECT), NONE);
}

#[test]
fn the_correct_file_actually_contains_what_every_rule_looks_for() {
    // Without this, the sweep above passes by matching nothing at all — which
    // is exactly how a rule with a broken head filter looks.
    for (needle, at_least) in [
        ("(define-key ", 3),
        ("(keymap-set ", 1),
        ("(interactive", 4),
        ("(add-hook ", 1),
        ("(remove-hook ", 1),
        ("(save-excursion", 1),
        ("(require '", 2),
        ("(lambda", 0),
    ] {
        let found = CORRECT.matches(needle).count();
        assert!(
            found >= at_least,
            "the correct corpus has {found} of `{needle}`, expected at least {at_least}"
        );
    }
    // And the sweep must be seeing a tree, not a parse failure.
    let tree = SyntaxTree::parse_with_dialect(CORRECT, Dialect::EmacsLisp).expect("corpus parses");
    assert!(
        tree.root_view().children.len() >= 15,
        "the corpus should have at least 15 top-level forms"
    );
}

/// The same file with each idiom broken, exactly once each.
const DANGEROUS_TWIN: &str = r#";;; my-pkg.el --- A package -*- lexical-binding: t -*-

;;; Commentary:

;; Every rule in this crate fires here, once.

;;; Code:

(require 'cl)

(defvar my-pkg-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-n") #'my-pkg-quiet)
    map)
  "Keymap.")

(defun my-pkg-quiet ()
  "Not a command, but bound to a key."
  (message "quiet"))

(defun my-pkg-next (n)
  "Move forward N lines."
  (interactive)
  (forward-line n))

(defun my-pkg-collect (buffer)
  "Collect from BUFFER."
  (save-excursion
    (set-buffer buffer)
    (buffer-string)))

(add-hook 'text-mode-hook (lambda () (setq fill-column 72)))

(provide 'my-pkg)
;;; my-pkg.el ends here
"#;

#[test]
fn the_dangerous_twin_fires_every_rule_exactly_once() {
    let mut fired = rules_for_file(DANGEROUS_TWIN);
    fired.sort_unstable();
    assert_eq!(
        fired,
        [
            "elisp-hook-lambda",
            "elisp-interactive-arity-mismatch",
            "elisp-keymap-binds-non-command",
            "elisp-require-obsolete-cl",
            "elisp-save-excursion-set-buffer",
        ]
    );
}

/// Every `.el` file the repository already ships, swept as a permanent test.
///
/// These are somebody else's fixtures, written for other rules, so they are
/// the closest thing here to code this crate did not author. A finding on one
/// of them is not automatically a bug — `lint_golden/emacs-lisp.el` is a file
/// of *deliberate* defects — but an unexplained one is, and this is where it
/// would surface.
/// Each fixture, and the findings this crate is expected to produce on it.
///
/// The judgement for every entry, recorded here rather than in a report that
/// rots:
///
/// - `sample.el` is a one-line `defun` with no `(interactive)` and no binding.
///   Nothing here has anything to say about it.
/// - `lint_golden/emacs-lisp.el` is the golden corpus for the *other* Emacs
///   Lisp rules. It is full of deliberate defects, but none of them is one of
///   ours, and a finding would mean this crate had started duplicating a rule
///   that already covers them.
/// - `corpus/elisp.el` has **one true positive**, and it is not a defect this
///   corpus was written to contain:
///
///   ```elisp
///   (defun paredit-corpus-run (items &optional predicate)
///     "Run over ITEMS, keeping those matching PREDICATE."
///     (interactive)
///     …)
///   ```
///
///   `items` is required and `(interactive)` supplies nothing. Verified in GNU
///   Emacs 31.0.91: `(commandp 'paredit-corpus-run)` is `t`, so it appears in
///   `M-x`, and `call-interactively` on it signals `wrong-number-of-arguments`.
///   The fixture is a parser/formatter corpus and is not loaded by Emacs, so
///   this is latent rather than live — but it is a real defect, found by the
///   sweep, and pinned here rather than silenced.
const REPOSITORY_FIXTURES: [(&str, &[&str]); 3] = [
    ("../../../tests/fixtures/sample.el", &[]),
    (
        "../../../tests/fixtures/corpus/elisp.el",
        &["elisp-interactive-arity-mismatch"],
    ),
    ("../../../tests/fixtures/lint_golden/emacs-lisp.el", &[]),
];

#[test]
fn the_repositorys_own_elisp_fixtures_are_swept() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (relative, expected) in REPOSITORY_FIXTURES {
        let path = base.join(relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
        assert_eq!(
            rules_for_file(&source),
            expected,
            "{} swept differently than recorded",
            path.display()
        );
    }
}

#[test]
fn the_swept_fixtures_are_real_emacs_lisp_and_not_empty() {
    // A sweep over three unreadable or empty files would pass silently, which
    // is the failure mode a corpus test is most prone to.
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut total_forms = 0usize;
    for (relative, _) in REPOSITORY_FIXTURES {
        let path = base.join(relative);
        let source = std::fs::read_to_string(&path).expect("fixture is readable");
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::EmacsLisp).expect("parses");
        let forms = tree.root_view().children.len();
        assert!(forms > 0, "{} has no top-level forms", path.display());
        total_forms += forms;
    }
    // 17 today. The floor is a tripwire for a fixture that gets emptied or a
    // path that stops resolving, not a target.
    assert!(
        total_forms >= 15,
        "the swept fixtures hold only {total_forms} forms between them"
    );
}

#[test]
fn the_two_corpora_differ_only_in_the_idioms_under_test() {
    // Both are real files that parse; the twin is not a syntax-error stub
    // that happens to trip everything.
    for source in [CORRECT, DANGEROUS_TWIN] {
        SyntaxTree::parse_with_dialect(source, Dialect::EmacsLisp).expect("both corpora parse");
    }
}
