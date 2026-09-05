//! `symbol-function-fset-dynamic-name` detection: a function definition
//! installed under a name built by `intern` at run time.
//!
//! `(setf (symbol-function (intern (format nil "~A-handler" kind))) #'run)`
//! defines a function. Nothing in the file spells that function's name, so no
//! `grep`, no cross-reference index, and no `who-calls` will connect the
//! definition to any of its callers — and whatever computes the string decides
//! what gets defined.
//!
//! # What has to be true for a finding
//!
//! Both halves, together:
//!
//! 1. The form **installs a function definition**: `fset` or `defalias` in
//!    Emacs Lisp, or a `setf` whose place is `(symbol-function …)` (Common Lisp
//!    and Emacs Lisp) or `(fdefinition …)` (Common Lisp).
//! 2. The name is **built at run time**: an `(intern …)` or `(intern-soft …)`
//!    whose own argument is not a string literal.
//!
//! So the ordinary `(setf (symbol-function 'foo) #'bar)` and
//! `(defalias 'my-alias #'other)` are not reported — their names are right
//! there — and neither is `(fset (intern "constant-name") #'f)`, whose name a
//! search for `constant-name` finds in this very line.
//!
//! # What is deliberately not covered
//!
//! - **`(setf (macro-function …) …)`**, which is the same shape for macros. It
//!   is left out because it is outside what this rule's name claims; a rule
//!   that reports it should say so in its own name.
//! - **Where the interned string came from.** `(intern name)` and
//!   `(intern (format nil "~A-p" x))` are both reported, and neither is traced
//!   back to its source. Doing so would need the value table this package never
//!   asks for.
//!
//! # Relation to `eval-of-non-constant`
//!
//! In Common Lisp, `paredit-feature-lint-safety`'s `eval-of-non-constant`
//! separately reports the *inner* `(intern …)` node, because its argument is
//! computed. That is a different span and a different claim — "this intern's
//! input is unknowable" rather than "this definition has no findable name" —
//! and this rule anchors on the installing form, never on the `intern`. In
//! Emacs Lisp there is no overlap at all: that rule is Common Lisp only.
//!
//! # Scope
//!
//! Common Lisp and Emacs Lisp. Each spelling is offered only to the dialect
//! that has it: `fset` and `defalias` are Emacs Lisp functions with no CLHS
//! counterpart, and `fdefinition` is CLHS with no Emacs Lisp counterpart.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    builds_a_runtime_symbol_name, call_operator, for_each_evaluated_subview, is_unevaluated_at,
};

/// One function definition installed under a run-time name.
#[derive(Debug, Clone)]
pub struct SymbolFunctionFsetDynamicNameItem {
    /// The span of the whole installing form (`(setf …)`, `(fset …)`,
    /// `(defalias …)`) — never the inner `intern`, which is
    /// `eval-of-non-constant`'s span.
    pub span: ByteSpan,
    /// How the definition is installed: `fset`, `defalias`,
    /// `setf symbol-function`, or `setf fdefinition`.
    pub installer: String,
    /// The operator that builds the name: `intern` or `intern-soft`.
    pub name_builder: String,
}

impl Finding for SymbolFunctionFsetDynamicNameItem {
    fn kind(&self) -> &'static str {
        "symbol-function-fset-dynamic-name"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("installer={}", self.installer),
            format!("name_builder={}", self.name_builder),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("installer", json!(self.installer)),
            ("name_builder", json!(self.name_builder)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} defines a function whose name {} builds at run time, so no search connects this \
             definition to its callers",
            self.installer, self.name_builder
        )
    }
}

/// The `setf` places that install a *function* definition, per dialect.
///
/// `fdefinition` is CLHS and has no Emacs Lisp counterpart; `symbol-function`
/// is a place in both.
const fn function_places(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["symbol-function", "fdefinition"],
        Dialect::EmacsLisp => &["symbol-function"],
        _ => &[],
    }
}

/// The one-argument installers whose first argument is the function name.
///
/// Both are Emacs Lisp *functions* — `(fset SYMBOL DEFINITION)` and
/// `(defalias SYMBOL DEFINITION &optional DOCSTRING)` — so their arguments are
/// evaluated and `(fset (intern …) …)` is a real shape. Common Lisp has
/// neither.
const fn direct_installers(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::EmacsLisp => &["fset", "defalias"],
        _ => &[],
    }
}

/// Calls `visit` with each function name this form installs a definition
/// under, together with the label describing how.
///
/// Allocation-free: the labels are `'static`, and nothing is collected.
fn for_each_installed_name(
    view: &ExpressionView,
    dialect: Dialect,
    mut visit: impl FnMut(&ExpressionView, &'static str),
) {
    let Some(head) = list_head(view) else {
        return;
    };

    if symbol_in(head, direct_installers(dialect)) {
        if let Some(name) = view.children.get(1) {
            // `direct_installers` is a two-element table, so the label is
            // decided by which of the two matched.
            let label = if symbol_in(head, &["fset"]) {
                "fset"
            } else {
                "defalias"
            };
            visit(name, label);
        }
        return;
    }

    if !symbol_in(head, &["setf"]) {
        return;
    }
    // `(setf place value place value …)`: the places are the odd children.
    //
    // The place's head is read *once* per place. Asking `calls_any` twice —
    // once to accept the place and once to label it — doubled this rule's
    // measured cost on a clean file, where every `(setf (symbol-function 'f)
    // …)` is accepted and then found to have a literal name.
    let mut index = 1;
    while let Some(place) = view.children.get(index) {
        index += 2;
        if !is_paren_list(place) {
            continue;
        }
        let Some(place_head) = list_head(place) else {
            continue;
        };
        if !symbol_in(place_head, function_places(dialect)) {
            continue;
        }
        let label = if symbol_is(place_head, "fdefinition") {
            "setf fdefinition"
        } else {
            "setf symbol-function"
        };
        if let Some(name) = place.children.get(1) {
            visit(name, label);
        }
    }
}

/// Examines one installing form.
///
/// Returns an empty `Vec` — which allocates nothing — for the overwhelming
/// majority of nodes. Ordered cheapest-first: the structural tests are pointer
/// derefs, and [`is_unevaluated_at`] runs only once a finding is otherwise
/// certain.
#[must_use]
pub fn examine(
    tree: &SyntaxTree,
    view: &ExpressionView,
    dialect: Dialect,
) -> Vec<SymbolFunctionFsetDynamicNameItem> {
    let mut found = Vec::new();
    for_each_installed_name(view, dialect, |name, installer| {
        if !builds_a_runtime_symbol_name(name) {
            return;
        }
        found.push((installer, call_operator(name)));
    });
    if found.is_empty() {
        return Vec::new();
    }
    // The template case: `` `(fset (intern ,n) #'f) `` is a list being built.
    if is_unevaluated_at(tree, view.span) {
        return Vec::new();
    }
    found
        .into_iter()
        .filter_map(|(installer, name_builder)| {
            Some(SymbolFunctionFsetDynamicNameItem {
                span: view.span,
                installer: installer.to_owned(),
                name_builder: name_builder?,
            })
        })
        .collect()
}

/// Collects every run-time-named function definition in one file, with the
/// number of evaluated installing forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_symbol_function_fset_dynamic_name_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SymbolFunctionFsetDynamicNameItem>> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("installing_form_count", json!(0))],
        ));
    }

    let mut installing_form_count = 0_usize;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            let mut installs = false;
            for_each_installed_name(subview, dialect, |_, _| installs = true);
            if !installs {
                return;
            }
            installing_form_count += 1;
            violations.extend(examine(tree, subview, dialect));
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("installing_form_count", json!(installing_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::for_each_subview;

    /// Runs `examine` on the first form in the source that installs a function
    /// definition, found by an *unfiltered* walk so that quoted occurrences
    /// reach it too — which is exactly what the engine's dispatch does.
    fn examined(input: &str, dialect: Dialect) -> Vec<SymbolFunctionFsetDynamicNameItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let root = tree.root_view();
        let mut found = None;
        for_each_subview(&root, |view| {
            if found.is_some() {
                return;
            }
            let mut installs = false;
            for_each_installed_name(view, dialect, |_, _| installs = true);
            if installs {
                found = Some(examine(&tree, view, dialect));
            }
        });
        found.unwrap_or_default()
    }

    fn installer(input: &str, dialect: Dialect) -> Option<String> {
        examined(input, dialect)
            .first()
            .map(|item| item.installer.clone())
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_setf_of_symbol_function_with_a_built_name() {
        let found = examined(
            "(setf (symbol-function (intern (format nil \"~A-handler\" kind))) #'run)",
            Dialect::CommonLisp,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].installer, "setf symbol-function");
        assert_eq!(found[0].name_builder, "intern");
    }

    #[test]
    fn flags_a_setf_of_fdefinition_in_common_lisp() {
        assert_eq!(
            installer(
                "(setf (fdefinition (intern name)) #'run)",
                Dialect::CommonLisp
            ),
            Some("setf fdefinition".to_owned())
        );
    }

    #[test]
    fn flags_fset_and_defalias_in_emacs_lisp() {
        assert_eq!(
            installer(
                "(fset (intern (concat \"my-\" suffix)) #'run)",
                Dialect::EmacsLisp
            ),
            Some("fset".to_owned())
        );
        assert_eq!(
            installer(
                "(defalias (intern (format \"%s-p\" base)) #'run)",
                Dialect::EmacsLisp
            ),
            Some("defalias".to_owned())
        );
    }

    #[test]
    fn flags_an_intern_soft_built_name() {
        let found = examined("(fset (intern-soft name) #'run)", Dialect::EmacsLisp);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name_builder, "intern-soft");
    }

    #[test]
    fn flags_each_offending_place_of_a_multi_place_setf() {
        let found = examined(
            "(setf (symbol-function (intern a)) #'one (fdefinition (intern b)) #'two)",
            Dialect::CommonLisp,
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].installer, "setf symbol-function");
        assert_eq!(found[1].installer, "setf fdefinition");
    }

    #[test]
    fn reads_the_head_case_insensitively_and_through_a_package_qualifier() {
        assert_eq!(
            installer(
                "(CL:SETF (CL:SYMBOL-FUNCTION (CL:INTERN name)) #'run)",
                Dialect::CommonLisp
            ),
            Some("setf symbol-function".to_owned())
        );
    }

    #[test]
    fn the_message_names_the_installer_and_the_name_builder() {
        let found = examined("(fset (intern name) #'run)", Dialect::EmacsLisp);
        assert_eq!(
            found[0].message(),
            "fset defines a function whose name intern builds at run time, so no search connects \
             this definition to its callers"
        );
        assert_eq!(found[0].kind(), "symbol-function-fset-dynamic-name");
    }

    // -- near-miss negatives -------------------------------------------------

    /// The trap named in the rule's own brief: the ordinary, literal-named
    /// definition.
    #[test]
    fn does_not_flag_a_literal_name() {
        assert_eq!(
            installer("(setf (symbol-function 'foo) #'bar)", Dialect::CommonLisp),
            None
        );
        assert_eq!(
            installer("(defalias 'my-alias #'other)", Dialect::EmacsLisp),
            None
        );
        assert_eq!(
            installer("(fset 'my-alias #'other)", Dialect::EmacsLisp),
            None
        );
    }

    /// A name built from a string literal *is* findable — it is spelled right
    /// there.
    #[test]
    fn does_not_flag_an_intern_of_a_string_literal() {
        assert_eq!(
            installer("(fset (intern \"constant-name\") #'f)", Dialect::EmacsLisp),
            None
        );
        assert_eq!(
            installer(
                "(setf (symbol-function (intern \"CONSTANT\")) #'f)",
                Dialect::CommonLisp
            ),
            None
        );
    }

    #[test]
    fn does_not_flag_a_setf_of_some_other_place() {
        assert_eq!(
            installer(
                "(setf (gethash (intern name) table) v)",
                Dialect::CommonLisp
            ),
            None
        );
        // A *variable*, not a callable: a different claim and a different rule.
        assert_eq!(
            installer("(setf (symbol-value (intern name)) 1)", Dialect::CommonLisp),
            None
        );
    }

    /// The value side of a `setf` is not a place. Only the odd children are.
    #[test]
    fn does_not_read_the_value_side_as_a_place() {
        assert_eq!(
            installer(
                "(setf *saved* (symbol-function (intern name)))",
                Dialect::CommonLisp
            ),
            None
        );
    }

    /// Each dialect is offered only the spellings it has.
    #[test]
    fn does_not_claim_a_spelling_the_dialect_lacks() {
        // `fset`/`defalias` are Emacs Lisp; in Common Lisp a `(fset …)` call is
        // some library's own function.
        assert_eq!(
            installer("(fset (intern name) #'f)", Dialect::CommonLisp),
            None
        );
        assert_eq!(
            installer("(defalias (intern name) #'f)", Dialect::CommonLisp),
            None
        );
        // `fdefinition` is CLHS; Emacs Lisp has no such place.
        assert_eq!(
            installer("(setf (fdefinition (intern name)) #'f)", Dialect::EmacsLisp),
            None
        );
    }

    #[test]
    fn an_unmodelled_dialect_is_left_alone() {
        assert_eq!(installer("(fset (intern name) f)", Dialect::Clojure), None);
    }

    // -- the five quote shapes, plus the macro template ----------------------

    const SHAPE: &str = "(fset (intern name) #'run)";

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert_eq!(installer(&format!("'{SHAPE}"), Dialect::EmacsLisp), None);
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert_eq!(
            installer(&format!("(quote {SHAPE})"), Dialect::EmacsLisp),
            None
        );
    }

    #[test]
    fn does_not_flag_an_unescaped_backquote() {
        assert_eq!(installer(&format!("`{SHAPE}"), Dialect::EmacsLisp), None);
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert_eq!(
            installer(&format!("'(a ,{SHAPE})"), Dialect::EmacsLisp),
            None
        );
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            installer(&format!("`(a ,{SHAPE})"), Dialect::EmacsLisp),
            Some("fset".to_owned())
        );
    }

    /// The archetype this rule most needs to survive: a macro that *generates*
    /// exactly the shape it reports. The template is data.
    #[test]
    fn does_not_flag_a_backquoted_macro_template() {
        assert_eq!(
            installer(
                "(defmacro define-handler (name)\n  \
                 `(setf (symbol-function (intern (format nil \"~A-handler\" ,name)))\n     \
                 (lambda () nil)))",
                Dialect::CommonLisp
            ),
            None
        );
        assert_eq!(
            installer(
                "(defmacro my-define (suffix)\n  `(fset (intern (concat \"my-\" ,suffix)) #'run))",
                Dialect::EmacsLisp
            ),
            None
        );
    }

    #[test]
    fn does_not_flag_a_form_written_inside_a_string_literal() {
        assert_eq!(
            installer(
                "(defvar my-doc \"(fset (intern name) #'run)\")",
                Dialect::EmacsLisp
            ),
            None
        );
    }

    // -- the report ----------------------------------------------------------

    fn report(input: &str, dialect: Dialect) -> FileFindings<SymbolFunctionFsetDynamicNameItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        build_symbol_function_fset_dynamic_name_report(Path::new("test.el"), dialect, &tree)
            .expect("build report")
    }

    #[test]
    fn the_denominator_counts_every_evaluated_installing_form_scanned() {
        let built = report(
            "(fset (intern a) #'one)\n(fset 'literal #'two)\n(defalias 'other #'three)\n'(fset (intern b) #'four)\n",
            Dialect::EmacsLisp,
        );
        assert_eq!(built.summary, vec![("installing_form_count", json!(3))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let built = report(
            "(defun install (kind)\n  (fset (intern (concat \"h-\" kind)) #'run))\n",
            Dialect::EmacsLisp,
        );
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.text_columns(),
            vec![
                "installer=fset".to_owned(),
                "name_builder=intern".to_owned()
            ]
        );
        assert_eq!(
            finding.json_fields(),
            vec![
                ("installer", json!("fset")),
                ("name_builder", json!("intern"))
            ]
        );
    }

    #[test]
    fn a_non_modelled_dialect_is_reported_as_unmodelled() {
        let built = report("(fset (intern name) f)", Dialect::Clojure);
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
        assert_eq!(built.summary, vec![("installing_form_count", json!(0))]);
    }

    #[test]
    fn a_modelled_dialect_is_reported_as_modelled() {
        assert!(report("(fset 'a #'b)", Dialect::EmacsLisp).dialect_modelled);
        assert!(report("(setf (symbol-function 'a) #'b)", Dialect::CommonLisp).dialect_modelled);
    }
}
