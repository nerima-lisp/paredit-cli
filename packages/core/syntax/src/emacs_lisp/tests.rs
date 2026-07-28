use crate::definition::DefinitionCategory;
use crate::dialect::Dialect;
use crate::sexpr::SyntaxTree;

use super::*;

fn parse(input: &str) -> SyntaxTree {
    SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).expect("fixture parses")
}

#[test]
fn operator_heads_are_matched_case_sensitively() {
    // Emacs Lisp has no reader case folding: `LET` is a symbol a user could
    // define, and resolving it to the special form would let a rename or a
    // binding walk rewrite unrelated code.
    assert_eq!(
        EmacsLispOperator::from_head("let"),
        Some(EmacsLispOperator::Let)
    );
    assert_eq!(EmacsLispOperator::from_head("LET"), None);
    assert_eq!(EmacsLispOperator::from_head("Defun"), None);
}

#[test]
fn a_package_qualified_head_is_not_stripped() {
    // `cl:let` is one Emacs Lisp symbol whose name contains a colon, not a
    // qualified reference to `let`.
    assert_eq!(EmacsLispOperator::from_head("cl:let"), None);
    assert_eq!(EmacsLispOperator::from_head("common-lisp:defun"), None);
}

#[test]
fn a_user_symbol_that_merely_starts_with_cl_resolves_to_nothing() {
    // `cl-` is a prefix convention, not a namespace: stripping it would make
    // every `cl-`-prefixed helper in a package resolve to a special form.
    assert_eq!(EmacsLispOperator::from_head("cl-my-helper"), None);
    assert_eq!(EmacsLispOperator::from_head("cl-defun-ish"), None);
}

#[test]
fn the_subr_x_conditional_binding_family_is_recognized() {
    for head in [
        "if-let",
        "if-let*",
        "when-let",
        "when-let*",
        "and-let*",
        "while-let",
    ] {
        let operator = EmacsLispOperator::from_head(head).unwrap_or_else(|| panic!("{head}"));
        assert!(operator.is_binding_form(), "{head}");
    }
}

#[test]
fn if_let_bindings_stop_being_visible_at_the_else_branch() {
    let Some(EmacsLispBinderShape::PairList {
        body_start,
        body_end,
        ..
    }) = EmacsLispOperator::from_head("if-let*").and_then(EmacsLispOperator::binder_shape)
    else {
        panic!("if-let* is a pair-list binder");
    };

    assert_eq!(body_start, 2);
    assert_eq!(body_end, Some(3));

    // `when-let*` has no else branch, so its bindings reach the whole body.
    let Some(EmacsLispBinderShape::PairList { body_end, .. }) =
        EmacsLispOperator::from_head("when-let*").and_then(EmacsLispOperator::binder_shape)
    else {
        panic!("when-let* is a pair-list binder");
    };
    assert_eq!(body_end, None);
}

#[test]
fn let_star_is_sequential_and_letrec_is_recursive() {
    let visibility = |head: &str| match EmacsLispOperator::from_head(head)
        .and_then(EmacsLispOperator::binder_shape)
    {
        Some(EmacsLispBinderShape::PairList { visibility, .. }) => visibility,
        other => panic!("{head}: {other:?}"),
    };

    assert_eq!(visibility("let"), EmacsLispBindingVisibility::Parallel);
    assert_eq!(visibility("let*"), EmacsLispBindingVisibility::Sequential);
    assert_eq!(visibility("letrec"), EmacsLispBindingVisibility::Recursive);
}

#[test]
fn dlet_binds_dynamically_whatever_the_file_header_says() {
    let scope = |head: &str| {
        EmacsLispOperator::from_head(head)
            .map(EmacsLispOperator::binding_scope)
            .unwrap_or_else(|| panic!("{head}"))
    };

    assert_eq!(scope("dlet"), EmacsLispBindingScope::AlwaysDynamic);
    assert_eq!(scope("let"), EmacsLispBindingScope::FileDefault);
    assert_eq!(scope("cl-flet"), EmacsLispBindingScope::AlwaysLexical);
    // `cl.el`'s `flet` rebound the function cell for a dynamic extent, which
    // is why it was superseded rather than renamed.
    assert_eq!(scope("flet"), EmacsLispBindingScope::AlwaysDynamic);
}

#[test]
fn local_callable_visibility_distinguishes_the_three_cl_lib_forms() {
    let form = |head: &str| {
        EmacsLispOperator::from_head(head)
            .and_then(EmacsLispOperator::local_callable_form)
            .unwrap_or_else(|| panic!("{head}"))
    };

    assert!(!form("cl-flet").group_is_self_visible());
    assert!(!form("cl-flet").is_sequential());
    assert!(form("cl-flet*").is_sequential());
    assert!(form("cl-labels").group_is_self_visible());
    assert!(form("cl-macrolet").is_macro());
}

#[test]
fn a_macro_definition_does_not_accept_an_interactive_form() {
    let shape = |head: &str| {
        EmacsLispOperator::from_head(head)
            .and_then(EmacsLispOperator::callable_shape)
            .unwrap_or_else(|| panic!("{head}"))
    };

    assert!(shape("defun").accepts_interactive());
    // A macro is expanded, never called, so `(interactive)` at the head of
    // one is an ordinary call that happens to be first.
    assert!(!shape("defmacro").accepts_interactive());
    assert!(shape("defmacro").accepts_docstring());
    assert_eq!(shape("defun").arglist_child_index(), 2);
    assert_eq!(shape("lambda").arglist_child_index(), 1);
}

#[test]
fn cl_defmethod_finds_its_arglist_by_scanning_past_a_qualifier() {
    let method = EmacsLispOperator::from_head("cl-defmethod").expect("cl-defmethod");
    assert_eq!(method.callable_shape(), None);
    assert_eq!(
        method.definition_arglist_is_first_list_at_or_after(),
        Some(2)
    );

    let plain = EmacsLispOperator::from_head("defun").expect("defun");
    assert_eq!(plain.definition_arglist_is_first_list_at_or_after(), None);
}

#[test]
fn definition_categories_cover_the_variable_and_customization_families() {
    let category = |head: &str| {
        EmacsLispOperator::from_head(head)
            .and_then(EmacsLispOperator::definition_category)
            .unwrap_or_else(|| panic!("{head}"))
    };

    assert_eq!(category("defun"), DefinitionCategory::Function);
    assert_eq!(category("defsubst"), DefinitionCategory::Function);
    assert_eq!(category("defmacro"), DefinitionCategory::Macro);
    assert_eq!(category("defvar"), DefinitionCategory::Variable);
    assert_eq!(category("defvar-local"), DefinitionCategory::Variable);
    assert_eq!(category("defconst"), DefinitionCategory::Constant);
    assert_eq!(category("defcustom"), DefinitionCategory::Customization);
    assert_eq!(category("defface"), DefinitionCategory::Customization);
    assert_eq!(category("define-minor-mode"), DefinitionCategory::Mode);
    assert_eq!(category("cl-defstruct"), DefinitionCategory::Struct);
    assert_eq!(
        category("cl-defgeneric"),
        DefinitionCategory::GenericFunction
    );
    assert_eq!(category("cl-defmethod"), DefinitionCategory::Method);
    assert_eq!(category("define-error"), DefinitionCategory::Condition);
    assert_eq!(category("ert-deftest"), DefinitionCategory::Test);
}

#[test]
fn only_the_defvar_family_declares_a_name_dynamic() {
    for head in ["defvar", "defvar-local", "defconst", "defcustom"] {
        let operator = EmacsLispOperator::from_head(head).unwrap_or_else(|| panic!("{head}"));
        assert!(operator.declares_dynamic_variable(), "{head}");
    }
    for head in ["defun", "defmacro", "defface", "defgroup"] {
        let operator = EmacsLispOperator::from_head(head).unwrap_or_else(|| panic!("{head}"));
        assert!(!operator.declares_dynamic_variable(), "{head}");
    }
}

#[test]
fn dependency_forms_separate_eager_loads_from_deferred_ones() {
    let form = |head: &str| {
        EmacsLispOperator::from_head(head)
            .and_then(EmacsLispOperator::dependency_form)
            .unwrap_or_else(|| panic!("{head}"))
    };

    assert!(form("require").loads_eagerly());
    assert!(form("load-library").loads_eagerly());
    // `autoload` defers the load until the function is called, and
    // `declare-function` never loads anything — it only tells the byte
    // compiler to stop warning.
    assert!(!form("autoload").loads_eagerly());
    assert!(!form("declare-function").loads_eagerly());
    assert!(!form("provide").loads_eagerly());

    assert_eq!(form("require").designator_child_index(), 1);
    assert_eq!(form("autoload").designator_child_index(), 2);
}

#[test]
fn lexical_binding_is_read_from_the_first_line_only() {
    assert_eq!(
        emacs_lisp_file_header(";;; f.el --- x -*- lexical-binding: t; -*-\n(defun f ())")
            .lexical_binding(),
        EmacsLispLexicalBinding::Enabled
    );
    assert_eq!(
        emacs_lisp_file_header(";;; f.el -*- lexical-binding: nil -*-\n").lexical_binding(),
        EmacsLispLexicalBinding::DisabledExplicitly
    );
    assert_eq!(
        emacs_lisp_file_header(";;; f.el --- x\n(defun f ())").lexical_binding(),
        EmacsLispLexicalBinding::Absent
    );
    // Emacs reads this setting from line 1 and nowhere else: by the time a
    // `Local Variables:` block is reached the file has already been read.
    assert_eq!(
        emacs_lisp_file_header("(defun f ())\n;; Local Variables:\n;; lexical-binding: t\n")
            .lexical_binding(),
        EmacsLispLexicalBinding::Absent
    );
}

#[test]
fn a_shebang_line_defers_the_header_to_the_second_line() {
    let header =
        emacs_lisp_file_header("#!/usr/bin/emacs --script\n;;; -*- lexical-binding: t -*-\n");
    assert_eq!(header.lexical_binding(), EmacsLispLexicalBinding::Enabled);
    let span = header.lexical_binding_span().expect("span");
    assert!(span.start().get() > "#!/usr/bin/emacs --script\n".len());
}

#[test]
fn a_mode_only_header_leaves_lexical_binding_absent() {
    assert_eq!(
        emacs_lisp_file_header(";;; -*- emacs-lisp -*-\n").lexical_binding(),
        EmacsLispLexicalBinding::Absent
    );
    assert_eq!(
        emacs_lisp_file_header(";;; -*- mode: emacs-lisp; coding: utf-8 -*-\n").lexical_binding(),
        EmacsLispLexicalBinding::Absent
    );
}

#[test]
fn any_non_nil_lexical_binding_value_enables_it() {
    // Emacs tests the value for non-nil rather than for `t`.
    for value in ["t", "1", "yes"] {
        assert_eq!(
            emacs_lisp_file_header(&format!(";;; -*- lexical-binding: {value} -*-\n"))
                .lexical_binding(),
            EmacsLispLexicalBinding::Enabled,
            "{value}"
        );
    }
}

#[test]
fn autoload_cookies_are_found_only_on_their_own_line() {
    let tree = parse(";;;###autoload\n(defun f ())\n(defun g ()) ;;;###autoload\n");
    let cookies = emacs_lisp_autoload_cookies(&tree);

    assert_eq!(cookies.len(), 1);
    assert!(cookies[0].is_standard());
    assert_eq!(cookies[0].payload(), EmacsLispAutoloadPayload::NextForm);
}

#[test]
fn a_cookie_inside_a_string_is_not_a_cookie() {
    // A package that documents its own cookie in a docstring must not be
    // reported as autoloading the next form.
    let tree = parse("(defun f ()\n  \";;;###autoload\"\n  nil)\n");
    assert!(emacs_lisp_autoload_cookies(&tree).is_empty());
}

#[test]
fn a_cookie_carrying_a_form_is_distinguished_from_a_bare_one() {
    let tree = parse(";;;###autoload (autoload 'f \"lib\")\n(defun g ())\n");
    let cookies = emacs_lisp_autoload_cookies(&tree);

    assert_eq!(cookies.len(), 1);
    // The inline form is what gets copied into loaddefs; the following
    // definition is *not* autoloaded, which is the trap this distinction
    // exists to expose.
    assert_eq!(cookies[0].payload(), EmacsLispAutoloadPayload::InlineForm);
}

#[test]
fn a_package_specific_cookie_prefix_is_reported_rather_than_ignored() {
    let tree = parse(";;;###org-autoload\n(defun f ())\n");
    let cookies = emacs_lisp_autoload_cookies(&tree);

    assert_eq!(cookies.len(), 1);
    assert!(!cookies[0].is_standard());
    assert_eq!(cookies[0].prefix(), "org-");
}

#[test]
fn a_word_that_merely_starts_with_autoload_is_not_a_cookie() {
    let tree = parse(";;;###autoloading\n(defun f ())\n");
    assert!(emacs_lisp_autoload_cookies(&tree).is_empty());
}

#[test]
fn symbol_prefix_matching_respects_component_boundaries() {
    assert!(emacs_lisp_symbol_has_prefix("magit-status", "magit"));
    assert!(emacs_lisp_symbol_has_prefix("magit--status", "magit-"));
    // `magistrate` shares no name component with `magit`.
    assert!(!emacs_lisp_symbol_has_prefix("magistrate", "magit"));
    assert!(!emacs_lisp_symbol_has_prefix("magit", "magit"));
}

#[test]
fn the_double_hyphen_convention_marks_a_private_name() {
    assert!(is_emacs_lisp_internal_symbol_name("foo--helper"));
    assert!(!is_emacs_lisp_internal_symbol_name("foo-helper"));
    // A leading `--` has no package prefix in front of it, so it does not
    // mark the name private to anything.
    assert!(!is_emacs_lisp_internal_symbol_name("--foo"));
}

#[test]
fn both_predicate_spellings_are_recognized() {
    assert!(is_emacs_lisp_predicate_name("buffer-live-p"));
    assert!(is_emacs_lisp_predicate_name("stringp"));
    assert!(!is_emacs_lisp_predicate_name("buffer-live"));
    assert!(!is_emacs_lisp_predicate_name("p"));
}

#[test]
fn reserved_prefixes_are_the_ones_emacs_itself_owns() {
    assert!(is_emacs_lisp_reserved_prefix("cl-defun"));
    assert!(is_emacs_lisp_reserved_prefix("emacs-version"));
    assert!(!is_emacs_lisp_reserved_prefix("magit-status"));
}

#[test]
fn known_control_forms_evaluate_their_subforms_where_they_are_written() {
    for head in [
        "progn",
        "with-temp-buffer",
        "save-excursion",
        "unwind-protect",
        "pcase",
        "eval-when-compile",
    ] {
        let operator = EmacsLispOperator::from_head(head).unwrap_or_else(|| panic!("{head}"));
        assert!(operator.evaluates_subforms_in_place(), "{head}");
    }

    // A binding form is not in that set: it opens a scope, so its subforms
    // are walked by the binder rather than in place.
    let binding = EmacsLispOperator::from_head("let").expect("let");
    assert!(!binding.evaluates_subforms_in_place());
}
