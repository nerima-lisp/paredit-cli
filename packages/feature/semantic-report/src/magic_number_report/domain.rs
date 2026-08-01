//! Numeric literals inside a function or method body that read like they
//! should have a name.
//!
//! A `defconstant`/`defparameter`/`defvar` (and their Emacs Lisp cousins)
//! exists precisely to give a magic number a name; a literal appearing
//! anywhere else in a function or method body has not had that done for it.
//! This report finds those literals so they can be extracted.
//!
//! Two things are deliberately never flagged.
//!
//! A small allow-list of universally idiomatic numbers — `0`, `1`, `-1`, `2`
//! — is common enough (loop starts, halving, negation, off-by-one arithmetic)
//! that naming every occurrence would be noise, not signal.
//!
//! The literal that *is* a constant/parameter/variable definition's own value
//! is the definition, not a use of one — `(defconstant +limit+ 10)`'s `10`
//! must not be reported as a magic number needing a name, because it already
//! has one three tokens to its left.
//!
//! Scanning is scoped to a function or method's body: a literal outside any
//! callable definition is not "used" anywhere yet, so there is nothing to
//! extract it *out of*.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_semantics::semantics::value::LiteralValue;
use paredit_core_semantics::semantics::value::policy::supports_value_propagation;
use paredit_core_semantics::semantics::value::service::literal_value;
use paredit_core_syntax::common_lisp::{
    common_lisp_reader_conditional_kind, common_lisp_reader_label_kind,
    normalize_common_lisp_operator_head,
};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value as JsonValue, json};

/// Integers idiomatic enough in their own right that naming them would be
/// noise: loop starts and array indices (`0`), increment/decrement and unary
/// identities (`1`), negation (`-1`), and halving/doubling/pairing (`2`).
const IDIOMATIC_INTEGERS: [i128; 4] = [0, 1, -1, 2];

/// One numeric literal in a function or method body with no name of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicNumberItem {
    pub span: ByteSpan,
    /// The literal exactly as written — `#x1F`, `10.`, `1.5d0` — not its
    /// evaluated value, so a fix can paste it straight back into a
    /// `defconstant` without a spelling change.
    pub literal: String,
    /// The name of the innermost function/method definition this literal
    /// was found inside. Always present today, because scanning never
    /// descends outside one; kept optional because "not inside any
    /// definition" is a real, reportable state and a consumer should not
    /// have to special-case an empty string to see it.
    pub enclosing_definition: Option<String>,
}

impl Finding for MagicNumberItem {
    fn kind(&self) -> &'static str {
        "magic-number"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.literal.clone(),
            format!("in={}", self.enclosing_definition.as_deref().unwrap_or("-")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, JsonValue)> {
        vec![
            ("literal", json!(self.literal)),
            ("enclosing_definition", json!(self.enclosing_definition)),
        ]
    }
}

#[must_use]
pub fn build_magic_number_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<MagicNumberItem> {
    let source = tree.source();
    // The value layer's own gate: it is what `literal_value` is drawn from,
    // and it happens to land on exactly Common Lisp and Emacs Lisp — the two
    // dialects this report starts with.
    let dialect_modelled = supports_value_propagation(dialect);

    let mut findings = Vec::new();
    let mut examined = 0usize;

    if dialect_modelled {
        for form in &tree.root_view().children {
            let Some(head) = list_head(form) else {
                continue;
            };
            let Some(shape) = definition_shape(dialect, form, head) else {
                continue;
            };
            if !is_callable_body(shape.category) {
                continue;
            }
            let Some(name) = shape.name(form) else {
                continue;
            };
            for body_form in shape.body_forms(form) {
                walk_body(dialect, body_form, name, &mut findings, &mut examined);
            }
        }
    }

    let flagged = findings.len();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect_modelled,
        source,
        findings,
        vec![
            ("numeric_literal_count", json!(examined)),
            ("magic_number_count", json!(flagged)),
        ],
    )
}

/// Whether a definition in this category has a body worth scanning for
/// magic numbers.
///
/// Deliberately narrower than [`DefinitionCategory::is_callable`]: a macro's
/// body produces code rather than running as code, and a generic function's
/// "body" is method-combination options, not executable forms, so neither
/// belongs here.
const fn is_callable_body(category: DefinitionCategory) -> bool {
    matches!(
        category,
        DefinitionCategory::Function | DefinitionCategory::Method
    )
}

/// Whether this category's definition form binds a name to a value rather
/// than to code — `defconstant`/`defparameter`/`defvar` in Common Lisp,
/// `defconst`/`defvar`/`defcustom` in Emacs Lisp.
///
/// [`DefinitionCategory::Parameter`] (Common Lisp `defparameter`) and
/// [`DefinitionCategory::Customization`] (Emacs Lisp `defcustom`) are
/// included alongside [`DefinitionCategory::Constant`] and
/// [`DefinitionCategory::Variable`]: all four are a name given to a value,
/// which is exactly the naming this report proposes elsewhere as a fix, so
/// none of them can be a magic number themselves.
const fn is_value_binding(category: DefinitionCategory) -> bool {
    matches!(
        category,
        DefinitionCategory::Constant
            | DefinitionCategory::Variable
            | DefinitionCategory::Parameter
            | DefinitionCategory::Customization
    )
}

/// Common Lisp heads that `CommonLispOperator::definition_category` maps to a
/// value-binding [`DefinitionCategory`] — see
/// `packages/core/syntax/src/common_lisp/operator/table.rs`'s
/// `definition_category`. `defcustom` has no Common Lisp equivalent, so it is
/// deliberately absent here even though it is a value binding in Emacs Lisp.
const COMMON_LISP_VALUE_BINDING_HEADS: [&str; 5] = [
    "defvar",
    "defglobal",
    "define-symbol-macro",
    "defconstant",
    "defparameter",
];

/// Emacs Lisp heads that `EmacsLispOperator::definition_category` maps to a
/// value-binding [`DefinitionCategory`] — see
/// `packages/core/syntax/src/emacs_lisp/operator/classify.rs`'s
/// `definition_category`. Emacs Lisp has no `DefinitionCategory::Parameter`
/// producer, so nothing here corresponds to it.
const EMACS_LISP_VALUE_BINDING_HEADS: [&str; 11] = [
    "defvar",
    "defvar-local",
    "defvar-keymap",
    "defconst",
    "defvaralias",
    "define-obsolete-variable-alias",
    "defcustom",
    "defface",
    "defgroup",
    "deftheme",
    "define-widget",
];

/// A cheap, non-allocating pre-check for whether `head` could plausibly be a
/// value-binding definition, before paying for [`definition_shape`]'s
/// classification.
///
/// `walk_body` calls this on every `List`-kind node in a scanned body — an
/// arbitrary function call as often as an actual definition — so it must not
/// allocate. [`definition_shape`]'s Common Lisp path
/// (`CommonLispOperator::from_head`) lowercases the normalized head into a
/// fresh `String` before matching; this instead reuses
/// [`normalize_common_lisp_operator_head`], which only strips a known package
/// prefix and returns a borrowed `&str`, and compares case-insensitively with
/// [`str::eq_ignore_ascii_case`].
///
/// This only needs to be a superset of the heads [`is_value_binding`] cares
/// about: a false positive here still falls through to the real
/// [`definition_shape`] check below and is rejected there, so behavior is
/// unchanged. Only `walk_body`'s dialects — Common Lisp and Emacs Lisp, per
/// `supports_value_propagation` — need an entry; every other dialect never
/// reaches this function.
fn head_may_be_value_binding(dialect: Dialect, head: &str) -> bool {
    match dialect {
        Dialect::CommonLisp => {
            let unqualified = normalize_common_lisp_operator_head(head);
            COMMON_LISP_VALUE_BINDING_HEADS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(unqualified))
        }
        // Emacs Lisp definition heads match case-sensitively — see
        // `EmacsLispOperator::from_head`'s doc comment — so no case-folding
        // or prefix-stripping applies here.
        Dialect::EmacsLisp => EMACS_LISP_VALUE_BINDING_HEADS.contains(&head),
        _ => false,
    }
}

/// Walks a function/method body, reporting every numeric literal that is not
/// idiomatic and not itself the direct value of a nested value-binding
/// definition.
///
/// `enclosing` names the innermost function/method definition, carried down
/// unchanged — a `flet`/`labels` clause nested inside is not tracked
/// separately, so a magic number inside one is still attributed to the
/// definition a reader would open to find it.
fn walk_body(
    dialect: Dialect,
    view: &ExpressionView,
    enclosing: &str,
    found: &mut Vec<MagicNumberItem>,
    examined: &mut usize,
) {
    if opens_unevaluated_context(view) {
        return;
    }

    if view.kind == ExpressionKind::Atom {
        record_if_numeric(dialect, view, enclosing, found, examined);
        return;
    }

    if view.kind != ExpressionKind::List {
        return;
    }

    if let Some(head) = list_head(view) {
        let shape = if head_may_be_value_binding(dialect, head) {
            definition_shape(dialect, view, head)
        } else {
            None
        };
        if let Some(shape) = shape {
            if is_value_binding(shape.category) {
                let value_form = shape.body_forms(view).first();
                for child in &view.children {
                    let is_direct_value_atom = value_form
                        .is_some_and(|value| std::ptr::eq(value, child))
                        && child.kind == ExpressionKind::Atom;
                    if is_direct_value_atom {
                        // The definition itself — not a magic-number use.
                        continue;
                    }
                    walk_body(dialect, child, enclosing, found, examined);
                }
                return;
            }
        }
    }

    for child in &view.children {
        walk_body(dialect, child, enclosing, found, examined);
    }
}

fn record_if_numeric(
    dialect: Dialect,
    view: &ExpressionView,
    enclosing: &str,
    found: &mut Vec<MagicNumberItem>,
    examined: &mut usize,
) {
    let Some(value) = literal_value(dialect, view) else {
        return;
    };
    if !matches!(value, LiteralValue::Integer(_) | LiteralValue::Float(_)) {
        return;
    }
    *examined += 1;
    if is_idiomatic(&value) {
        return;
    }
    found.push(MagicNumberItem {
        span: view.span,
        literal: view.text.clone().unwrap_or_default(),
        enclosing_definition: Some(enclosing.to_owned()),
    });
}

fn is_idiomatic(value: &LiteralValue) -> bool {
    matches!(value, LiteralValue::Integer(n) if IDIOMATIC_INTEGERS.contains(n))
}

/// Whether this form and everything under it denotes itself rather than the
/// value of the code it is written as.
///
/// Mirrors `constant_report::domain::opens_unevaluated_context`: a quoted or
/// quasiquoted numeral is data, not a use of a magic number in running code,
/// and a reader conditional or reader label's contents depend on something
/// this layer cannot see.
fn opens_unevaluated_context(view: &ExpressionView) -> bool {
    view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::Quote
                | ReaderPrefix::Quasiquote
                | ReaderPrefix::ReadEval
                | ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
        )
    }) || common_lisp_reader_conditional_kind(view).is_some()
        || common_lisp_reader_label_kind(view).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_of(source: &str, dialect: Dialect) -> FileFindings<MagicNumberItem> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_magic_number_report(Path::new("t.lisp"), dialect, &tree)
    }

    fn report(source: &str) -> FileFindings<MagicNumberItem> {
        report_of(source, Dialect::CommonLisp)
    }

    #[test]
    fn a_literal_in_a_function_body_is_flagged() {
        let report = report("(defun f (x) (+ x 42))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].literal, "42");
        assert_eq!(
            report.findings[0].enclosing_definition.as_deref(),
            Some("f")
        );
    }

    #[test]
    fn idiomatic_numbers_are_never_flagged() {
        let report = report("(defun f (x) (if (= x -1) 0 (* x 2)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn every_idiomatic_number_is_still_counted_as_examined() {
        let report = report("(defun f (x) (if (= x -1) 0 (* x 2)))");
        assert_eq!(report.summary[0], ("numeric_literal_count", json!(3)));
        assert_eq!(report.summary[1], ("magic_number_count", json!(0)));
    }

    #[test]
    fn a_constants_own_direct_value_is_not_a_magic_number() {
        let report = report("(defconstant +limit+ 100)");
        assert!(report.findings.is_empty(), "{report:?}");
        assert_eq!(report.summary[0], ("numeric_literal_count", json!(0)));
    }

    #[test]
    fn a_defparameters_own_direct_value_is_not_a_magic_number() {
        let report = report("(defparameter *limit* 100)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_defvars_own_direct_value_is_not_a_magic_number() {
        let report = report("(defvar *limit* 100)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_magic_number_beside_a_constant_definition_is_still_flagged() {
        let report = report("(defconstant +limit+ 100)\n(defun f (x) (+ x 42))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].literal, "42");
    }

    #[test]
    fn nested_arithmetic_can_flag_more_than_one_literal() {
        let report = report("(defun f () (* 60 60))");
        assert_eq!(report.findings.len(), 2, "{report:?}");
        let literals = report
            .findings
            .iter()
            .map(|item| item.literal.as_str())
            .collect::<Vec<_>>();
        assert_eq!(literals, vec!["60", "60"]);
    }

    #[test]
    fn a_method_body_is_scanned_like_a_function_body() {
        let report = report("(defmethod render ((node widget)) (+ node 42))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(
            report.findings[0].enclosing_definition.as_deref(),
            Some("render")
        );
    }

    #[test]
    fn a_generic_function_declaration_is_not_scanned() {
        let report = report("(defgeneric render (node) (:documentation \"42\"))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_top_level_literal_outside_any_definition_is_not_flagged() {
        let report = report("(print 42)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn quoted_data_is_not_flagged() {
        let report = report("(defun f () '(1 2 42))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_float_literal_is_flagged_with_no_idiomatic_exception() {
        let report = report("(defun f () (* 1.5 2))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].literal, "1.5");
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let report = report_of("(defn f [x] (+ x 42))", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_emacs_lisp_file_is_modelled_and_flags_its_own_arithmetic() {
        let report = report_of("(defun f (x) (+ x 42))", Dialect::EmacsLisp);
        assert!(report.dialect_modelled);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].literal, "42");
    }

    #[test]
    fn an_emacs_lisp_defconsts_own_direct_value_is_not_a_magic_number() {
        let report = report_of("(defconst my-limit 100)", Dialect::EmacsLisp);
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_emacs_lisp_defcustoms_own_direct_value_is_not_a_magic_number() {
        let report = report_of(
            "(defcustom my-limit 100 \"doc\" :type 'integer)",
            Dialect::EmacsLisp,
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun f () (list 42 43 44))");
        let starts = report
            .findings
            .iter()
            .map(|item| item.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn text_columns_and_json_fields_carry_the_literal_and_enclosing_definition() {
        let report = report("(defun f (x) (+ x 42))");
        let finding = &report.findings[0];
        assert_eq!(finding.text_columns()[0], "42");
        assert_eq!(finding.text_columns()[1], "in=f");
        let fields = finding.json_fields();
        assert_eq!(fields[0], ("literal", json!("42")));
        assert_eq!(fields[1], ("enclosing_definition", json!("f")));
    }
}
