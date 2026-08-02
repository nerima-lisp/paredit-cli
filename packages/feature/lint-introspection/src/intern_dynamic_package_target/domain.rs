//! `intern-dynamic-package-target` detection: `(intern "NAME" <computed>)`,
//! where the *package* argument is an expression rather than a package the
//! source names.
//!
//! `intern` puts a symbol into a package. With a literal package designator —
//! `(intern "HANDLER" :app)`, `(intern "HANDLER" (find-package :app))` — a
//! reader can follow the symbol to the package it lands in, and so can a
//! cross-reference tool. With a computed one — `(intern "HANDLER"
//! (find-package (format nil "APP/~A" module)))` — the destination is decided at
//! run time, the symbol is not where any static search will look for it, and a
//! package that does not exist yields `nil`, which `intern` then signals on.
//!
//! # What is deliberately not flagged
//!
//! - **No package argument at all.** `(intern name)` interns into `*package*`,
//!   which is the documented default and not a computed target.
//! - **`*package*` written out.** `(intern "X" *package*)` says exactly what the
//!   default says.
//! - **A literal designator**, in any of its four spellings: a string
//!   (`"APP"`), a keyword (`:app`), a quoted symbol (`'app`), and any of those
//!   wrapped in `find-package`.
//! - **A bare variable.** `(intern "X" package)` inside a helper whose caller
//!   passes the package is a *parameterized* function, not a computed target,
//!   and reporting it would fire on every such utility. Only a package argument
//!   that is itself a **call** is reported. A deliberate false negative.
//! - **A package carried by an argument.** `(intern "SETTER" (symbol-package
//!   sym))` and `(intern "X" (package-name p))` re-derive a destination the
//!   caller already supplied rather than choosing one; the first is how an
//!   accessor generator puts a new name beside an existing one. The same
//!   deliberate false negative as the bare variable, one level in.
//! - **A computed *name*.** `(intern name pkg)` is already
//!   `paredit-feature-lint-safety`'s `eval-of-non-constant`, which anchors on
//!   the same `intern` head, reports the same span, and fires exactly when the
//!   first argument is not a literal the source shows. Requiring the name here
//!   to be a string literal makes the two rules' triggers **disjoint by
//!   construction** rather than two names for one finding. The consequence is
//!   stated plainly: a call whose name *and* package are both computed is
//!   reported once, by that rule, under its name.
//!
//! # Scope
//!
//! Common Lisp only. Emacs Lisp's `intern` takes an *obarray*, not a package,
//! and passing a computed obarray is ordinary Emacs Lisp rather than a smell.
//! Clojure's `(intern ns name val)` takes its namespace *first* and additionally
//! creates a Var binding — it is closer to `def` than to CLHS `intern`, so
//! modelling it under this rule's sentence would be describing a different
//! operation.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    atom_is, call_operator, calls_any, for_each_evaluated_subview, is_keyword_atom, is_quoted_form,
    is_string_literal, is_unevaluated_at,
};

/// One `intern` whose package argument is computed.
#[derive(Debug, Clone)]
pub struct InternDynamicPackageTargetItem {
    /// The span of the whole `(intern …)` form.
    pub span: ByteSpan,
    /// The symbol name being interned, exactly as the source spells it,
    /// including its quotes.
    pub name: String,
    /// The operator of the computed package expression, which is what a reader
    /// has to go and read to find out where the symbol lands.
    pub package_operator: String,
}

impl Finding for InternDynamicPackageTargetItem {
    fn kind(&self) -> &'static str {
        "intern-dynamic-package-target"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("name={}", self.name),
            format!("package_operator={}", self.package_operator),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("package_operator", json!(self.package_operator)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "intern chooses its package with the computed expression ({} …), so nothing in the \
             source says which package {} is interned into",
            self.package_operator, self.name
        )
    }
}

/// Whether a form names a package the source shows outright.
///
/// The four spellings a literal package designator has — a string, a keyword, a
/// quoted symbol, and `*package*` — plus `find-package` wrapped around any of
/// them, which is how a package object is usually obtained from a literal name.
fn is_literal_package_designator(view: &ExpressionView) -> bool {
    if is_string_literal(view) || is_keyword_atom(view) || atom_is(view, "*package*") {
        return true;
    }
    // `'app` and `(quote app)` both name a package the source shows. Checked
    // before the call test below, because `(quote app)` *is* a `(…)` list.
    if is_quoted_form(view) {
        return true;
    }
    calls_any(view, &["find-package"])
        && view
            .children
            .get(1)
            .is_some_and(is_literal_package_designator)
}

/// Calls that *carry* a package rather than choose one.
///
/// `(intern "SETTER" (symbol-package sym))` interns beside a symbol the caller
/// supplied; `(intern "X" (package-name p))` re-spells a package the caller
/// supplied. Neither call decides anything — the destination travelled in with
/// the argument, exactly as it does for the bare variable in
/// `(intern "X" package)`, and both are ordinary accessor-generating idiom.
///
/// Exempting them is the same deliberate false negative as the bare-variable
/// case, one level in.
const CARRIED_PACKAGE_OPERATORS: [&str; 2] = ["symbol-package", "package-name"];

/// The operator of a package argument that is computed, or `None` when the
/// argument names a package statically.
///
/// Only a *call* counts. A bare symbol is a parameter, not a computation — see
/// this module's header for why that is a deliberate false negative.
///
/// That restriction is enforced by [`call_operator`] alone, and deliberately so
/// rather than by an explicit `is_paren_list` test in front of it:
/// `view_query::list_head`, which `call_operator` is built on, *opens* with
/// `is_paren_list(view).then(…)`, so an atom yields `None` there already. An
/// added check was carried here until a mutation run showed that removing it
/// broke no test — it was unreachable by construction, and a guard that cannot
/// fail is a guard a later reader will trust for a job it is not doing.
/// `does_not_flag_a_bare_variable_package` is what holds the behaviour.
fn computed_package_operator(view: &ExpressionView) -> Option<String> {
    if is_literal_package_designator(view) || calls_any(view, &CARRIED_PACKAGE_OPERATORS) {
        return None;
    }
    call_operator(view)
}

/// Examines one `(intern …)` form.
///
/// Ordered cheapest-first: the structural tests are a handful of pointer
/// derefs and allocate nothing, and [`is_unevaluated_at`] — the only part that
/// touches the tree — runs last, once the finding is otherwise certain.
#[must_use]
pub fn examine(tree: &SyntaxTree, view: &ExpressionView) -> Option<InternDynamicPackageTargetItem> {
    if !calls_any(view, &["intern"]) {
        return None;
    }
    let name = view.children.get(1)?;
    // No package argument is `*package*`, which is the documented default.
    let package = view.children.get(2)?;

    // Disjointness with `eval-of-non-constant`: it owns every `intern` whose
    // *name* the source does not show.
    if !is_string_literal(name) {
        return None;
    }
    let package_operator = computed_package_operator(package)?;

    // The template case: `` `(intern "X" ,pkg) `` is a list being built.
    if is_unevaluated_at(tree, view.span) {
        return None;
    }

    Some(InternDynamicPackageTargetItem {
        span: view.span,
        name: name.span.slice(tree.source()).to_owned(),
        package_operator,
    })
}

/// Collects every `intern` with a computed package target in one file, with the
/// number of evaluated `intern` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no computed package target here" for
/// Common Lisp and "nothing was looked for" for Emacs Lisp, and the two read
/// identically without the flag.
pub fn build_intern_dynamic_package_target_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<InternDynamicPackageTargetItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("intern_form_count", json!(0))],
        ));
    }

    let mut intern_form_count = 0_usize;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            if !calls_any(subview, &["intern"]) {
                return;
            }
            intern_form_count += 1;
            if let Some(item) = examine(tree, subview) {
                violations.push(item);
            }
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("intern_form_count", json!(intern_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::for_each_subview;

    /// Runs `examine` on the first `(intern …)` form in the source, found by an
    /// *unfiltered* walk so that quoted occurrences reach it too — which is
    /// exactly what the engine's dispatch does.
    fn examined(input: &str) -> Option<InternDynamicPackageTargetItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = None;
        for_each_subview(&root, |view| {
            if found.is_none() && calls_any(view, &["intern"]) {
                found = examine(&tree, view);
            }
        });
        found
    }

    fn package_operator(input: &str) -> Option<String> {
        examined(input).map(|item| item.package_operator)
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_package_looked_up_by_a_computed_name() {
        let found = examined("(intern \"HANDLER\" (find-package (format nil \"APP/~A\" m)))")
            .expect("a finding");
        assert_eq!(found.package_operator, "find-package");
        assert_eq!(found.name, "\"HANDLER\"");
    }

    #[test]
    fn flags_any_other_computed_package_expression() {
        assert_eq!(
            package_operator("(intern \"*STATE*\" (package-of thing))"),
            Some("package-of".to_owned())
        );
        assert_eq!(
            package_operator("(intern \"X\" (make-package name))"),
            Some("make-package".to_owned())
        );
    }

    #[test]
    fn reads_the_head_case_insensitively_and_through_a_package_qualifier() {
        assert_eq!(
            package_operator("(CL:INTERN \"X\" (CL:FIND-PACKAGE name))"),
            Some("find-package".to_owned())
        );
    }

    #[test]
    fn the_message_names_both_the_symbol_and_the_computation() {
        let found = examined("(intern \"HANDLER\" (find-package name))").expect("a finding");
        assert_eq!(
            found.message(),
            "intern chooses its package with the computed expression (find-package …), so \
             nothing in the source says which package \"HANDLER\" is interned into"
        );
        assert_eq!(found.kind(), "intern-dynamic-package-target");
    }

    // -- near-miss negatives -------------------------------------------------

    /// The trap named in the rule's own brief: the documented default, written
    /// out.
    #[test]
    fn does_not_flag_the_current_package() {
        assert_eq!(package_operator("(intern \"X\" *package*)"), None);
        assert_eq!(package_operator("(intern \"X\" CL:*PACKAGE*)"), None);
    }

    #[test]
    fn does_not_flag_a_call_with_no_package_argument() {
        assert_eq!(package_operator("(intern \"X\")"), None);
        assert_eq!(package_operator("(intern)"), None);
    }

    #[test]
    fn does_not_flag_a_literal_package_designator() {
        assert_eq!(package_operator("(intern \"X\" :app)"), None);
        assert_eq!(package_operator("(intern \"X\" \"APP\")"), None);
        assert_eq!(package_operator("(intern \"X\" 'app)"), None);
        assert_eq!(package_operator("(intern \"X\" (quote app))"), None);
    }

    #[test]
    fn does_not_flag_find_package_of_a_literal() {
        assert_eq!(package_operator("(intern \"X\" (find-package :app))"), None);
        assert_eq!(
            package_operator("(intern \"X\" (find-package \"APP\"))"),
            None
        );
        assert_eq!(package_operator("(intern \"X\" (find-package 'app))"), None);
    }

    /// The parameterized helper: a package the caller chose, not one this call
    /// computed. A deliberate false negative.
    #[test]
    fn does_not_flag_a_bare_variable_package() {
        assert_eq!(package_operator("(intern \"X\" package)"), None);
        assert_eq!(package_operator("(intern \"X\" *app-package*)"), None);
    }

    /// The same case one level in: the destination travelled in with the
    /// argument. `(intern "…" (symbol-package sym))` is how an accessor
    /// generator puts a new name beside an existing one, and is ordinary
    /// idiom rather than a computed target.
    #[test]
    fn does_not_flag_a_package_carried_by_its_argument() {
        assert_eq!(
            package_operator("(intern \"SETTER\" (symbol-package sym))"),
            None
        );
        assert_eq!(
            package_operator("(intern \"SETTER\" (symbol-package 'bar))"),
            None
        );
        assert_eq!(package_operator("(intern \"X\" (package-name p))"), None);
    }

    /// The other half of that exemption: a package *chosen* by a call is still
    /// reported, so the carve-out above cannot be silently widened.
    #[test]
    fn still_flags_a_package_the_call_chooses() {
        assert_eq!(
            package_operator("(intern \"X\" (find-package (config :pkg)))"),
            Some("find-package".to_owned())
        );
    }

    /// The disjointness guard. Both of these have a computed package, and both
    /// are `eval-of-non-constant`'s finding rather than this rule's, because
    /// the *name* is computed too.
    #[test]
    fn defers_a_computed_name_to_eval_of_non_constant() {
        assert_eq!(package_operator("(intern name (find-package p))"), None);
        assert_eq!(
            package_operator("(intern (string-upcase n) (find-package p))"),
            None
        );
    }

    // -- the five quote shapes, plus the macro template ----------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert_eq!(
            package_operator("'(intern \"X\" (find-package name))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert_eq!(
            package_operator("(quote (intern \"X\" (find-package name)))"),
            None
        );
    }

    #[test]
    fn does_not_flag_an_unescaped_backquote() {
        assert_eq!(
            package_operator("`(intern \"X\" (find-package name))"),
            None
        );
    }

    /// A comma inside a *hard* quote is a literal comma, so the form is still
    /// data — the shape a single depth counter reads wrongly.
    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert_eq!(
            package_operator("'(a ,(intern \"X\" (find-package name)))"),
            None
        );
    }

    /// The one shape that is code again.
    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            package_operator("`(a ,(intern \"X\" (find-package name)))"),
            Some("find-package".to_owned())
        );
    }

    /// The risk this whole package is written around: a macro whose expansion
    /// contains the shape. The `intern` is template text, not a call.
    #[test]
    fn does_not_flag_a_backquoted_macro_template() {
        assert_eq!(
            package_operator(
                "(defmacro define-slot (name module)\n  \
                 `(intern \"HANDLER\" (find-package (format nil \"APP/~A\" ,module))))"
            ),
            None
        );
    }

    #[test]
    fn does_not_flag_a_form_written_inside_a_string_literal() {
        assert_eq!(
            package_operator("(defparameter *doc* \"(intern \\\"X\\\" (find-package p))\")"),
            None
        );
    }

    // -- the report ----------------------------------------------------------

    fn report(input: &str) -> FileFindings<InternDynamicPackageTargetItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        build_intern_dynamic_package_target_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    #[test]
    fn the_denominator_counts_every_evaluated_intern_form_scanned() {
        let built = report(
            "(intern \"A\" (find-package p))\n(intern \"B\" :app)\n(intern \"C\")\n'(intern \"D\" (find-package q))\n",
        );
        assert_eq!(built.summary, vec![("intern_form_count", json!(3))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let built = report("(defun f (m)\n  (intern \"HANDLER\" (find-package m)))\n");
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.text_columns(),
            vec![
                "name=\"HANDLER\"".to_owned(),
                "package_operator=find-package".to_owned(),
            ]
        );
        assert_eq!(
            finding.json_fields(),
            vec![
                ("name", json!("\"HANDLER\"")),
                ("package_operator", json!("find-package")),
            ]
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(intern \"X\" (find-package p))", Dialect::EmacsLisp)
                .expect("parse");
        let built = build_intern_dynamic_package_target_report(
            Path::new("app.el"),
            Dialect::EmacsLisp,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
        assert_eq!(built.summary, vec![("intern_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(intern \"X\" :app)").dialect_modelled);
    }
}
