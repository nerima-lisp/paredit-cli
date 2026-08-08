use super::Formatter;
use crate::clojure::{ClojureIndentStyle, clojure_indent_style_for_head};
use crate::common_lisp::{CommonLispOperator, normalize_common_lisp_operator_head};
use crate::dialect::Dialect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ListStyle {
    Definition,
    SystemDefinition,
    Defmethod,
    DefinitionNameBody,
    Lambda,
    NamedLambda,
    Binding,
    LocalFunctions,
    OneArgumentBody,
    TwoArgumentBody,
    ClauseForm,
    CondClauses,
    CaseClauses,
    Do,
    Prog,
    Declaration,
    PairAssignment,
    Loop,
    HeadBody,
    If,
    /// `if` in Common Lisp: the test is on the head line and all branches
    /// (then, else) align at the same distinguished column (two indent
    /// steps from the form's opening delimiter — the equivalent of Emacs
    /// `common-lisp-indent-function`'s `(&rest nil)` for `if`).
    IfAligned,
    /// `(defn name [params]` with the body indented below, falling back to only
    /// the name on the head line when a docstring, attribute map, or multi-arity
    /// clause list follows it.
    ClojureDefinition,
    /// `(fn [params]` or `(fn name [params]` with the body indented below.
    ClojureFunction,
    /// The head plus this many children share the head line and every remaining
    /// child is indented one per line.
    ///
    /// `ClojurePrefixBody(0)` is the head-alone layout, so Clojure does not need
    /// a separate [`ListStyle::HeadBody`] mapping.
    ClojurePrefixBody(usize),
    /// `(-> value` with one threading step per line, aligned under the value.
    /// The payload is how many children share the head line.
    ClojureThreading(usize),
    /// One test/result pair per line. The payload is how many children precede
    /// the clauses and stay on the head line.
    ClojurePairClauses(usize),
    General,
}

/// The vocabulary a project's `paredit.toml` may use in `format.indent-table`
/// and `format.width-profiles` (see [`Formatter::with_indent_overrides`] and
/// [`Formatter::with_width_profiles`]).
///
/// Deliberately narrower than [`ListStyle`]: every entry here has a single,
/// generic, dialect-independent behavior a plain string can name without
/// ambiguity. Left out, each for its own reason:
///
/// - Every `Clojure*` variant — dialect-specific, and three of them
///   (`ClojureThreading`, `ClojurePrefixBody`, `ClojurePairClauses`) carry an
///   internal `usize` a bare style name has nowhere to put.
/// - [`ListStyle::SystemDefinition`] — the ASDF `defsystem` keyword/value
///   plist header is narrow and Common-Lisp-specific enough that retargeting
///   an unrelated symbol onto it would almost certainly be a mistake, not an
///   intentional override.
/// - [`ListStyle::Defmethod`] — CLOS method syntax has an optional qualifier
///   before the lambda list that plain [`ListStyle::Definition`] does not;
///   an arbitrary symbol given this style would have that slot misread.
/// - [`ListStyle::NamedLambda`] — renders identically to
///   [`ListStyle::TwoArgumentBody`]/[`ListStyle::If`] (all three call
///   `format_prefix_body` with the same prefix length); a second name for
///   the same behavior is a synonym, not a capability worth the surface area.
/// - [`ListStyle::LocalFunctions`] — the `flet`/`labels` nested
///   binding-list-of-lists shape is specific to that pair of operators.
/// - [`ListStyle::ClauseForm`], [`ListStyle::Do`], [`ListStyle::Prog`] —
///   `do`/`prog`'s bindings/end-test/body shape is a fixed positional
///   structure, not a generic "body" a `defun`-like symbol would want.
/// - [`ListStyle::Declaration`], [`ListStyle::PairAssignment`] —
///   `declare`/`setq`'s pairing shape is specific to those forms.
/// - [`ListStyle::Loop`] — has its own internal loop-keyword clause grammar,
///   unrelated to every other style here.
pub const STYLE_NAMES: &[&str] = &[
    "general",
    "definition",
    "name-body",
    "lambda",
    "binding-list",
    "one-argument-body",
    "two-argument-body",
    "if-then-else",
    "if-aligned",
    "cond-clauses",
    "case-clauses",
    "head-body",
];

/// Resolves one of [`STYLE_NAMES`] to the style it names. `None` when `name`
/// is not in that list.
pub(super) fn style_from_name(name: &str) -> Option<ListStyle> {
    match name {
        "general" => Some(ListStyle::General),
        "definition" => Some(ListStyle::Definition),
        "name-body" => Some(ListStyle::DefinitionNameBody),
        "lambda" => Some(ListStyle::Lambda),
        "binding-list" => Some(ListStyle::Binding),
        "one-argument-body" => Some(ListStyle::OneArgumentBody),
        "two-argument-body" => Some(ListStyle::TwoArgumentBody),
        "if-then-else" => Some(ListStyle::If),
        "if-aligned" => Some(ListStyle::IfAligned),
        "cond-clauses" => Some(ListStyle::CondClauses),
        "case-clauses" => Some(ListStyle::CaseClauses),
        "head-body" => Some(ListStyle::HeadBody),
        _ => None,
    }
}

impl Formatter {
    pub(super) fn style_for_head(&self, head: &str) -> ListStyle {
        // A project's `paredit.toml` entry always wins over the built-in
        // per-dialect table for the same symbol — the configuration layer
        // this codebase treats as highest-precedence everywhere else (see
        // `packages/core/config/src/load.rs`). Searched in reverse so that,
        // per this struct's `indent_overrides` field doc, a later entry for
        // the same symbol wins over an earlier one — the same "later beats
        // earlier" precedence `load.rs` uses across configuration layers.
        if let Some((_, style)) = self
            .indent_overrides
            .iter()
            .rev()
            .find(|(symbol, _)| symbol == head)
        {
            return *style;
        }
        match self.dialect {
            Dialect::Clojure => Self::clojure_style_for_head(head),
            Dialect::EmacsLisp => Self::elisp_style_for_head(head),
            _ => Self::common_lisp_style_for_head(head),
        }
    }

    /// Maps [`ClojureIndentStyle`] onto the formatter's rendering styles.
    ///
    /// `Call` must map to [`ListStyle::General`], because [`Formatter::inline_list`]
    /// and [`Formatter::compact_node`] refuse to compact any list whose head
    /// carries a style. Giving ordinary calls a style of their own would stop
    /// every Clojure form from ever fitting on one line.
    fn clojure_style_for_head(head: &str) -> ListStyle {
        match clojure_indent_style_for_head(head) {
            ClojureIndentStyle::Definition => ListStyle::ClojureDefinition,
            ClojureIndentStyle::Function => ListStyle::ClojureFunction,
            // A Clojure binding vector lays out exactly like a Common Lisp
            // binding list: the vector stays on the head line with one
            // name/value pair per line, and the body is indented below it.
            ClojureIndentStyle::BindingVector => ListStyle::Binding,
            ClojureIndentStyle::Threading(prefix) => ListStyle::ClojureThreading(prefix),
            ClojureIndentStyle::PairClauses(prefix) => ListStyle::ClojurePairClauses(prefix),
            ClojureIndentStyle::Body(prefix) => ListStyle::ClojurePrefixBody(prefix),
            ClojureIndentStyle::HeadBody => ListStyle::ClojurePrefixBody(0),
            ClojureIndentStyle::Call => ListStyle::General,
        }
    }

    /// Emacs Lisp dialect routing: recognizes Elisp-specific operators that
    /// differ from Common Lisp conventions.  Falls back to the CL table for
    /// everything else.
    fn elisp_style_for_head(head: &str) -> ListStyle {
        let normalized_head = normalize_common_lisp_operator_head(head);
        match normalized_head.to_ascii_lowercase().as_str() {
            // def* forms with 'defun indent → name on head line, body indented
            "defvar" | "defconst" | "defcustom" | "defgroup" | "defalias"
            | "defvaralias" | "define-derived-mode" | "define-minor-mode" => {
                ListStyle::DefinitionNameBody
            }
            // progn-like (indent 0) → all children at body-indent
            "save-excursion" | "save-restriction" | "save-current-buffer"
            | "track-mouse" => ListStyle::HeadBody,
            // indent 1 → one arg on head line, body indented
            "while" => ListStyle::OneArgumentBody,
            // indent 2 → two-component special (then at +4, else at +2)
            "condition-case" => ListStyle::TwoArgumentBody,
            // if in Elisp: test at +4 distinguished, then/else at body
            "if" => ListStyle::If,
            // Elisp-specific clause forms
            "pcase" => ListStyle::CaseClauses,
            "cl-loop" => ListStyle::Loop,
            _ => Self::common_lisp_style_for_head(head),
        }
    }

    fn common_lisp_style_for_head(head: &str) -> ListStyle {
        if let Some(operator) = CommonLispOperator::from_head(head) {
            match operator {
                operator if operator.is_method_definition() => {
                    return ListStyle::Defmethod;
                }
                CommonLispOperator::Lambda => return ListStyle::Lambda,
                CommonLispOperator::DefineSymbolMacro => return ListStyle::DefinitionNameBody,
                operator if operator.is_asdf_system_definition() => {
                    return ListStyle::SystemDefinition;
                }
                operator if operator.definition_category().is_some() => {
                    return ListStyle::Definition;
                }
                operator if operator.is_let_binding() || operator.is_handler_bind_binding() => {
                    return ListStyle::Binding;
                }
                operator if operator.is_local_callable_binding() => {
                    return ListStyle::LocalFunctions;
                }
                operator if operator.is_iteration_binding() || operator.is_slot_binding() => {
                    return ListStyle::OneArgumentBody;
                }
                operator if operator.is_value_binding() => return ListStyle::TwoArgumentBody,
                operator if operator.is_clause_binding() => return ListStyle::ClauseForm,
                operator if operator.is_do_binding() => return ListStyle::Do,
                operator if operator.is_prog_binding() => return ListStyle::Prog,
                CommonLispOperator::Loop => return ListStyle::Loop,
                _ => {}
            }
        }

        let normalized_head = normalize_common_lisp_operator_head(head);
        match normalized_head.to_ascii_lowercase().as_str() {
            "named-lambda" => ListStyle::NamedLambda,
            "if" => ListStyle::IfAligned,
            "when"
            | "unless"
            | "with-open-file"
            | "with-open-stream"
            | "with-input-from-string"
            | "with-output-to-string"
            | "with-hash-table-iterator"
            | "with-package-iterator"
            | "block"
            | "catch"
            | "unwind-protect"
            | "eval-when" => ListStyle::OneArgumentBody,
            "cond" => ListStyle::CondClauses,
            "case" | "ccase" | "ecase" | "typecase" | "ctypecase" | "etypecase" => {
                ListStyle::CaseClauses
            }
            "progn" | "prog1" | "prog2" | "tagbody" | "defpackage" | "locally" => {
                ListStyle::HeadBody
            }
            "declare" | "declaim" | "proclaim" => ListStyle::Declaration,
            "setq" | "psetq" | "setf" | "psetf" | "multiple-value-setq"
            | "multiple-value-setf" => ListStyle::PairAssignment,
            "multiple-value-call" | "multiple-value-prog1" | "pprint-logical-block"
            | "with-compilation-unit" | "with-standard-io-syntax"
            | "return-from" | "throw" => ListStyle::OneArgumentBody,
            "progv" | "with-condition-restarts" | "print-unreadable-object" => {
                ListStyle::TwoArgumentBody
            }
            "generic-flet" | "generic-labels" => ListStyle::LocalFunctions,
            _ => ListStyle::General,
        }
    }
}
