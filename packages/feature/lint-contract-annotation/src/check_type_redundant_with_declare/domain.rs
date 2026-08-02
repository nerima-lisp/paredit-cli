//! Common Lisp `check-type`-restating-`declare` detection: a runtime type
//! assertion guarding a variable whose type an adjacent `declare` has already
//! promised.
//!
//! # Why this is a defect and not belt-and-braces
//!
//! The two forms make opposite claims about who is responsible.
//!
//! CLHS, `type` declaration: "During the execution of any reference to the
//! declared variable within the scope of the declaration, the consequences are
//! undefined if the value of the declared variable is not of the declared
//! type."
//!
//! CLHS, `check-type`: it "signals a correctable error of type `type-error` if
//! the contents of place are not of the type typespec", and "can return only if
//! the `store-value` restart is invoked".
//!
//! Put them next to each other and the `check-type` cannot do its job. It is
//! itself *a reference to the declared variable*, so by the time it reads the
//! variable the program has already entered undefined behaviour in exactly the
//! case the check exists to catch. An implementation that takes the declaration
//! at its word — which the standard entitles it to — may fold the test away
//! entirely. And `store-value`, the restart that gives `check-type` its whole
//! value, would be repairing a variable the declaration already promised was
//! fine.
//!
//! So one of the two is wrong, and which one is the author's call: drop the
//! declaration so the check can actually run, or drop the check because the
//! declaration made it dead. That is why this is [`Fixability::ReportOnly`].
//!
//! # Only `declare`, never `the`
//!
//! An earlier sketch of this rule also compared against an adjacent
//! `(the integer x)`. That half was dropped as incoherent: `the` wraps an
//! *expression* and constrains the values it returns, whereas `declare`
//! constrains a *variable* over a scope. There is no well-defined notion of
//! "the `the` for this variable" to compare a `check-type` against, and
//! guessing at one would be guessing.
//!
//! # Disjoint from the semantic type report
//!
//! `paredit-feature-semantic-report`'s `type_report` already puts a binding's
//! *declared* type beside its *inferred* one and reports the pair when their
//! meet is the empty type — a declaration no object can satisfy. It treats
//! `declare`, `declaim`, `proclaim` and `check-type` as writing the same slot,
//! so when both say `integer` their meet is `integer` and it reports nothing at
//! all. This rule is exactly that gap: not a contradiction between two claims,
//! but the same claim made twice in two mechanisms that cannot both be right
//! about who checks it.
//!
//! # What is deliberately not flagged
//!
//! - **A differing type.** `(declare (type integer x))` beside
//!   `(check-type x (integer 0 10))` is a narrowing, not a restatement, and is
//!   left alone. Only an exact restatement is reported.
//! - **A non-variable place.** `(check-type (car xs) integer)` names a place a
//!   `declare` cannot describe.
//! - **A `declare` that is not a sibling.** Only the `check-type`'s immediate
//!   parent is read. A declaration in an enclosing `defun` with the check
//!   inside a nested `when` is a false negative, on purpose: scanning the whole
//!   ancestor chain once per `check-type` is the shape that makes a rule
//!   quadratic inside one large function.
//! - **Declaration identifiers that are not types** — `ignore`, `optimize`,
//!   `special`, `ftype`, `inline` and the rest of the standard set are excluded
//!   from the short form `(typespec var*)`.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::policy::RuleDialectScope;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, symbol_is, unqualified};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, head_is, is_unevaluated_at, with_leading_declarations,
};

/// The one place this rule's dialect is decided; both the engine's
/// `dialect_scope` and the standalone report's `dialect_modelled` read it.
pub const SCOPE: RuleDialectScope = RuleDialectScope::COMMON_LISP_ONLY;

/// The standard declaration identifiers, which are *not* type specifiers.
///
/// Needed for the short form: CLHS says "(typespec var*) is an abbreviation for
/// (type typespec var*)", so `(declare (integer x))` is a type declaration —
/// but `(declare (ignore x))` is not, and telling them apart is exactly this
/// list. `declaration` and `type` are included so neither is ever read as a
/// type name.
const DECLARATION_IDENTIFIERS: [&str; 11] = [
    "declaration",
    "dynamic-extent",
    "ftype",
    "ignorable",
    "ignore",
    "inline",
    "notinline",
    "optimize",
    "special",
    "type",
    "values",
];

#[derive(Debug, Clone)]
pub struct CheckTypeRedundantWithDeclareItem {
    /// The span of the `check-type` form, which is what a reader must decide
    /// about.
    pub span: ByteSpan,
    /// The span of the `declare` that already promised the same type.
    pub declaration_span: ByteSpan,
    pub variable: String,
    /// The type specifier, in the normalized spelling both sides were compared
    /// in.
    pub type_spec: String,
}

impl Finding for CheckTypeRedundantWithDeclareItem {
    fn kind(&self) -> &'static str {
        "check-type-redundant-with-declare"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("variable={}", self.variable),
            format!("type={}", self.type_spec),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("variable", json!(self.variable)),
            ("type", json!(self.type_spec)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "check-type on `{}` restates the adjacent (declare (type {} {})): the declaration \
             already makes it undefined behaviour for `{}` to hold another type, so the check \
             cannot be relied on to fire — drop the declaration to keep the check, or drop the \
             check because the declaration made it dead",
            self.variable, self.type_spec, self.variable, self.variable
        )
    }
}

/// A type specifier rendered in one canonical spelling, so that `(INTEGER 0 10)`
/// and `(integer 0 10)` compare equal — the Common Lisp reader upcases, and a
/// package qualifier does not change which type is named.
///
/// Recursive over a compound specifier's shape rather than a source slice, so
/// that whitespace and line breaks do not make two identical specifiers differ.
fn type_spec_text(view: &ExpressionView) -> Option<String> {
    if let Some(text) = atom_text(view) {
        return Some(unqualified(text).to_ascii_lowercase());
    }
    if !is_paren_list(view) {
        return None;
    }
    let parts: Option<Vec<String>> = view.children.iter().map(type_spec_text).collect();
    Some(format!("({})", parts?.join(" ")))
}

/// One `(type typespec var*)` or `(typespec var*)` entry of a `declare`, as
/// `(type-text, variable-names)`.
fn read_declaration_entry(entry: &ExpressionView) -> Option<(String, Vec<String>)> {
    if !is_paren_list(entry) {
        return None;
    }
    let head = entry.children.first()?;
    let head_text = atom_text(head)?;

    let (spec, variables) = if symbol_is(head_text, "type") {
        // `(type typespec var*)`
        (entry.children.get(1)?, &entry.children[2..])
    } else {
        // `(typespec var*)`, but only when the head is not one of the standard
        // declaration identifiers.
        let normalized = unqualified(head_text).to_ascii_lowercase();
        if DECLARATION_IDENTIFIERS.contains(&normalized.as_str()) {
            return None;
        }
        (head, &entry.children[1..])
    };

    let names: Vec<String> = variables
        .iter()
        .filter_map(atom_text)
        .map(|name| unqualified(name).to_ascii_lowercase())
        .collect();
    if names.is_empty() {
        return None;
    }
    Some((type_spec_text(spec)?, names))
}

/// The `(place, typespec)` of a `(check-type place typespec)` whose place is a
/// plain variable.
fn read_check_type(view: &ExpressionView) -> Option<(String, String)> {
    if !head_is(view, "check-type") {
        return None;
    }
    // `(check-type place typespec [string])`
    if view.children.len() < 3 {
        return None;
    }
    // A `declare` can only describe a variable, so a general place —
    // `(check-type (car xs) integer)` — has nothing to compare against.
    let place = atom_text(view.children.get(1)?)?;
    let spec = type_spec_text(view.children.get(2)?)?;
    Some((unqualified(place).to_ascii_lowercase(), spec))
}

/// Examines one `check-type`, reading the `declare` forms at the head of the
/// body that encloses it.
///
/// Takes the tree because a `check-type` cannot see its own parent: the
/// `RuleContext` carries no parent pointer. The lookup walks down by path,
/// reading only spans and head symbols, and materializes nothing but the
/// `declare` forms themselves — so its cost does not depend on how large the
/// enclosing function is. Materializing the enclosing top-level form per
/// candidate instead measured 4.6 seconds on a function with 2000 checks.
pub fn examine_check_type(
    tree: &SyntaxTree,
    view: &ExpressionView,
    check_type_count: &mut usize,
    violations: &mut Vec<CheckTypeRedundantWithDeclareItem>,
) {
    let Some((variable, spec)) = read_check_type(view) else {
        return;
    };
    *check_type_count += 1;

    let found = with_leading_declarations(tree, view.span, |declaration| {
        for entry in declaration.children.get(1..)? {
            let Some((declared_spec, names)) = read_declaration_entry(entry) else {
                continue;
            };
            if declared_spec == spec && names.contains(&variable) {
                return Some((declaration.span, declared_spec));
            }
        }
        None
    });

    if let Some((declaration_span, type_spec)) = found {
        violations.push(CheckTypeRedundantWithDeclareItem {
            span: view.span,
            declaration_span,
            variable,
            type_spec,
        });
    }
}

/// Collects every `check-type` restating an adjacent `declare` in one file,
/// with the number of `check-type` forms scanned as the denominator.
pub fn build_check_type_redundant_with_declare_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CheckTypeRedundantWithDeclareItem>> {
    let modelled = SCOPE.includes(dialect);
    let mut check_type_count = 0;
    let mut violations = Vec::new();

    if modelled {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_check_type(tree, subview, &mut check_type_count, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("check_type_count", json!(check_type_count))],
    ))
}

/// The rule's own data guard, shared with the report so the two agree.
#[must_use]
pub fn is_data_at(tree: &SyntaxTree, span: ByteSpan) -> bool {
    is_unevaluated_at(tree, span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CheckTypeRedundantWithDeclareItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_check_type_redundant_with_declare_report(
            Path::new("app.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn findings(input: &str) -> Vec<CheckTypeRedundantWithDeclareItem> {
        report(input).findings
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_check_type_restating_a_declare() {
        let found = findings("(defun f (x) (declare (type integer x)) (check-type x integer) x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].variable, "x");
        assert_eq!(found[0].type_spec, "integer");
    }

    /// CLHS: "(typespec var*) is an abbreviation for (type typespec var*)".
    #[test]
    fn flags_the_short_form_declaration() {
        assert_eq!(
            findings("(defun f (x) (declare (integer x)) (check-type x integer) x)").len(),
            1
        );
    }

    #[test]
    fn flags_when_the_declaration_names_several_variables() {
        let found =
            findings("(defun f (x y) (declare (type integer x y)) (check-type y integer) (+ x y))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].variable, "y");
    }

    #[test]
    fn flags_a_compound_type_specifier_restated_exactly() {
        assert_eq!(
            findings(
                "(defun f (x) (declare (type (integer 0 10) x)) (check-type x (integer 0 10)) x)"
            )
            .len(),
            1
        );
    }

    /// The Common Lisp reader upcases, and a package qualifier does not change
    /// which type is named, so both sides are compared case-folded and
    /// unqualified.
    #[test]
    fn the_comparison_folds_case_and_package_qualifiers() {
        assert_eq!(
            findings("(defun f (x) (declare (type CL:INTEGER x)) (check-type x integer) x)").len(),
            1
        );
    }

    /// Whitespace between the two spellings must not make them differ: the
    /// specifier is compared by shape, not by source slice.
    #[test]
    fn the_comparison_ignores_whitespace_in_a_compound_specifier() {
        assert_eq!(
            findings(
                "(defun f (x)\n  (declare (type (integer 0\n                          10) x))\n  \
                 (check-type x (integer 0 10))\n  x)"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_inside_a_let_body_too() {
        assert_eq!(
            findings("(let ((x 1)) (declare (type integer x)) (check-type x integer) x)").len(),
            1
        );
    }

    #[test]
    fn the_finding_carries_both_spans() {
        let source = "(defun f (x) (declare (type integer x)) (check-type x integer) x)";
        let found = findings(source);
        let check = &source[found[0].span.start().get()..found[0].span.end().get()];
        let declaration =
            &source[found[0].declaration_span.start().get()..found[0].declaration_span.end().get()];
        assert_eq!(check, "(check-type x integer)");
        assert_eq!(declaration, "(declare (type integer x))");
    }

    // -- near-miss negatives -------------------------------------------------

    /// The near-miss that matters most: a *narrower* check is not a
    /// restatement, and is the legitimate reason to write both.
    #[test]
    fn does_not_flag_a_narrower_check() {
        assert!(
            findings("(defun f (x) (declare (type integer x)) (check-type x (integer 0 10)) x)")
                .is_empty()
        );
        assert!(
            findings("(defun f (x) (declare (type number x)) (check-type x integer) x)").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_check_on_a_different_variable() {
        assert!(
            findings("(defun f (x y) (declare (type integer x)) (check-type y integer) y)")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_check_type_with_no_declaration_at_all() {
        assert!(findings("(defun f (x) (check-type x integer) x)").is_empty());
    }

    /// Only the immediate parent is read; a declaration further out is a
    /// documented false negative.
    #[test]
    fn does_not_read_a_declaration_from_an_enclosing_form() {
        assert!(
            findings(
                "(defun f (x) (declare (type integer x)) (when (plusp x) (check-type x integer)))"
            )
            .is_empty()
        );
    }

    /// The standard declaration identifiers are not type specifiers.
    #[test]
    fn does_not_read_a_non_type_declaration_as_a_type() {
        for declaration in [
            "(ignore x)",
            "(ignorable x)",
            "(special x)",
            "(dynamic-extent x)",
            "(inline x)",
            "(notinline x)",
            "(optimize speed)",
        ] {
            assert!(
                findings(&format!(
                    "(defun f (x) (declare {declaration}) (check-type x integer) x)"
                ))
                .is_empty(),
                "{declaration}"
            );
        }
    }

    /// The exclusion list above, pinned where it is actually *observable*.
    ///
    /// The `does_not_read_a_non_type_declaration_as_a_type` cases above pass
    /// even with the list deleted, because a finding also needs the declared
    /// text to equal the `check-type`'s type specifier — and `integer` never
    /// equals `ignore`. Removing the guard was caught by nothing until this
    /// test, which makes the two texts collide on purpose.
    ///
    /// Contrived Common Lisp, deliberately: the point is that a declaration
    /// identifier must never be read as a type name whatever it is compared
    /// against.
    #[test]
    fn a_declaration_identifier_is_never_read_as_a_type_even_when_the_text_collides() {
        for identifier in DECLARATION_IDENTIFIERS {
            // `type` is handled by the explicit branch above the short form, so
            // it can never reach the exclusion list; the other ten can.
            if identifier == "type" {
                continue;
            }
            let source =
                format!("(defun f (x) (declare ({identifier} x)) (check-type x {identifier}) x)");
            assert!(
                findings(&source).is_empty(),
                "`{identifier}` is a declaration identifier, not a type"
            );
        }
    }

    /// `ftype` describes a function, not a variable's value.
    #[test]
    fn does_not_read_an_ftype_declaration_as_a_variable_type() {
        assert!(
            findings(
                "(defun f (x) (declare (ftype (function (integer) integer) x)) \
                 (check-type x integer) x)"
            )
            .is_empty()
        );
    }

    /// A `declare` can only describe a variable, so a general place has nothing
    /// to compare against.
    #[test]
    fn does_not_flag_a_non_variable_place() {
        assert!(
            findings("(defun f (xs) (declare (type list xs)) (check-type (car xs) integer) xs)")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_malformed_check_type() {
        assert!(findings("(defun f (x) (declare (type integer x)) (check-type x))").is_empty());
    }

    #[test]
    fn does_not_flag_an_unrelated_head() {
        assert!(
            findings("(defun f (x) (declare (type integer x)) (assert (integerp x)) x)").is_empty()
        );
    }

    // -- the five quote shapes -----------------------------------------------

    const TRIGGER: &str = "(defun f (x) (declare (type integer x)) (check-type x integer) x)";

    #[test]
    fn a_quoted_form_is_data_and_is_not_flagged() {
        assert!(findings(&format!("'{TRIGGER}")).is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings(&format!("(quote {TRIGGER})")).is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(findings(&format!("'(a ,{TRIGGER})")).is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(findings(&format!("`{TRIGGER}")).is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(findings(&format!("`(a ,{TRIGGER})")).len(), 1);
    }

    // -- a string literal ----------------------------------------------------

    #[test]
    fn a_form_spelled_inside_a_string_is_text_not_a_form() {
        assert!(findings(&format!("(format t \"{}\")", TRIGGER.replace('"', ""))).is_empty());
    }

    // -- the wrong dialect ---------------------------------------------------

    /// The same bytes read as another dialect reach nothing, and the report
    /// says "not measured" rather than "clean".
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        for dialect in [Dialect::Clojure, Dialect::Racket, Dialect::Scheme] {
            let tree = SyntaxTree::parse_with_dialect(TRIGGER, dialect).expect("parse");
            let report =
                build_check_type_redundant_with_declare_report(Path::new("f.x"), dialect, &tree)
                    .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
            assert_eq!(report.summary, vec![("check_type_count", json!(0))]);
        }
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(defun f (x) x)").dialect_modelled);
    }

    // -- the envelope --------------------------------------------------------

    #[test]
    fn the_summary_counts_every_check_type_not_only_the_redundant_ones() {
        let report = report(
            "(defun a (x) (declare (type integer x)) (check-type x integer) x)\n\
             (defun b (x) (check-type x integer) x)\n\
             (defun c (x) (check-type x string) x)\n",
        );
        assert_eq!(report.summary, vec![("check_type_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report =
            report("(defun f (x)\n  (declare (type integer x))\n  (check-type x integer)\n  x)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "check-type-redundant-with-declare");
        assert_eq!(
            finding.json_fields(),
            vec![("variable", json!("x")), ("type", json!("integer"))]
        );
        assert!(finding.message().contains("undefined behaviour"));
    }
}
