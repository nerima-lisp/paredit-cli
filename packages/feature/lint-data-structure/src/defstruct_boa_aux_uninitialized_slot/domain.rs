//! `defstruct-boa-aux-uninitialized-slot` detection: a BOA constructor whose
//! `&aux` section names a slot with no value form, leaving that slot
//! uninitialized however carefully its `:initform` was written.
//!
//! # The premise this rule replaces, and why that one was wrong
//!
//! The obvious rule here is "a BOA lambda list that *omits* a slot skips that
//! slot's `:initform`". That is false, and CLHS says so directly under
//! `defstruct`:
//!
//! > If a slot is not initialized in this way, it is initialized by evaluating
//! > *slot-initform*.
//!
//! Running it confirms it. SBCL 2.6.0, with a BOA constructor naming neither
//! `label` nor `scale`:
//!
//! ```text
//! (defstruct (point (:constructor make-point (x y)))
//!   (x 0) (y 0) (label "none") (scale 1.0))
//! (make-point 1 2)
//! === P1a: BOA omits label and scale, both HAVE initforms ===
//! label="none" scale=1.0
//! ```
//!
//! Both initforms ran. A rule built on that premise would have fired on every
//! partial BOA constructor in existence, all of them correct.
//!
//! # What is actually broken
//!
//! `&aux` is the mechanism CLHS provides for *overriding* the default
//! initialization, and a bare `&aux` variable — one with no value form —
//! overrides it with nothing at all. The slot is then read before it is
//! written, which CLHS leaves undefined:
//!
//! > If no *slot-initform* is supplied, the consequences are undefined if an
//! > attempt is later made to read the slot's value before a value is
//! > explicitly assigned.
//!
//! SBCL does not leave it undefined; it traps, and names the slot:
//!
//! ```text
//! (defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 999))
//! (rec-b (make-rec 1))
//! === P1c: slot B has :initform 999 but appears as bare &aux ===
//! Unhandled SIMPLE-TYPE-ERROR: Accessed uninitialized slot B of structure REC
//! ```
//!
//! Note what that shows: `b` has an `:initform` of `999`, in the same form, and
//! it is *not* used. The bare `&aux` is the whole difference between this and
//! the correct code above.
//!
//! # What is not reported
//!
//! - **`&aux` with a value form.** `(:constructor make-rec (a &aux (b 5)))`
//!   initializes the slot to 5; that is the option working as designed, and it
//!   is verified below.
//! - **An `&aux` variable that is not a slot.** A BOA lambda list may bind
//!   ordinary temporaries, and `(:constructor make-x (a &aux (tmp (f a))))`
//!   names no slot. A bare non-slot `&aux` binds a local to `nil` and is
//!   nobody's bug.
//! - **A slot merely absent from the lambda list**, per the whole section
//!   above.
//! - **The default constructor.** `(:constructor)` and a `defstruct` with no
//!   `:constructor` option at all take keyword arguments and have no `&aux`.
//!
//! # Scope
//!
//! Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::atom_text;
use serde_json::{Value, json};

use crate::support::{self, DefstructForm, defstruct_form};

#[derive(Debug, Clone)]
pub struct DefstructBoaAuxUninitializedSlotItem {
    /// The span of the `&aux` variable itself, which is the token to change.
    pub span: ByteSpan,
    pub structure: String,
    pub slot: String,
    pub constructor: String,
    /// Whether the slot description supplies an `:initform` that this `&aux`
    /// is overriding. Both cases are defects; this one is the more surprising,
    /// because the initform is right there and does not run.
    pub overrides_initform: bool,
}

impl Finding for DefstructBoaAuxUninitializedSlotItem {
    fn kind(&self) -> &'static str {
        "defstruct-boa-aux-uninitialized-slot"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("slot={}", self.slot),
            format!("structure={}", self.structure),
            format!("constructor={}", self.constructor),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("slot", json!(self.slot)),
            ("structure", json!(self.structure)),
            ("constructor", json!(self.constructor)),
            ("overrides_initform", json!(self.overrides_initform)),
        ]
    }

    fn message(&self) -> String {
        let tail = if self.overrides_initform {
            format!(
                "the slot's own :initform is overridden and never evaluated, so reading {} \
                 before something assigns it has undefined consequences (SBCL: \"Accessed \
                 uninitialized slot\")",
                self.slot
            )
        } else {
            format!(
                "the slot has no :initform either, so reading {} before something assigns it \
                 has undefined consequences (SBCL: \"Accessed uninitialized slot\")",
                self.slot
            )
        };
        format!(
            "the BOA constructor {} binds the slot {} of {} as a bare &aux variable: {tail}; \
             give the &aux variable a value form, as in (&aux ({} …)), or drop it from the \
             lambda list to let the slot's :initform run",
            self.constructor, self.slot, self.structure, self.slot
        )
    }
}

/// Every `(:constructor name lambda-list)` in a `defstruct` header.
///
/// The two-element `(:constructor name)` form and the bare `(:constructor)`
/// both name the keyword constructor, which has no lambda list to read.
fn boa_constructors<'a>(
    form: &'a DefstructForm<'a>,
) -> impl Iterator<Item = (&'a str, &'a ExpressionView)> {
    form.option_forms()
        .filter(|option| {
            option
                .children
                .first()
                .and_then(atom_text)
                .is_some_and(|text| support::is_keyword(text, "constructor"))
        })
        .filter_map(|option| {
            let name = atom_text(option.children.get(1)?)?;
            let lambda_list = option.children.get(2)?;
            Some((name, lambda_list))
        })
}

/// The bare `&aux` variables of a BOA lambda list: the ones after `&aux` that
/// are plain symbols rather than `(name value)` pairs.
fn bare_aux_variables(lambda_list: &ExpressionView) -> Vec<&ExpressionView> {
    let mut seen_aux = false;
    let mut bare = Vec::new();
    for item in &lambda_list.children {
        match atom_text(item) {
            // A later lambda-list marker would end the &aux section, but &aux
            // is the last one CLHS allows, so reaching another means the
            // lambda list is malformed and reading on would be guesswork.
            Some(text) if text.starts_with('&') => {
                if text.eq_ignore_ascii_case("&aux") {
                    seen_aux = true;
                } else if seen_aux {
                    break;
                }
            }
            Some(_) if seen_aux => bare.push(item),
            // `(name value)` supplies a value; `(name)` does not, and is the
            // same defect spelled with parentheses.
            None if seen_aux && item.children.len() == 1 => bare.push(&item.children[0]),
            _ => {}
        }
    }
    bare
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Everything up to and including the `&aux` scan is local to this one
/// `defstruct` form — the constructor option and the slot list are operands of
/// the same node — so the quote check runs only once a real defect is in hand.
pub fn examine_defstruct_boa_aux_uninitialized_slot(
    tree: &SyntaxTree,
    view: &ExpressionView,
    defstruct_form_count: &mut usize,
    violations: &mut Vec<DefstructBoaAuxUninitializedSlotItem>,
) {
    let Some(form) = defstruct_form(view) else {
        return;
    };
    *defstruct_form_count += 1;

    let mut found = Vec::new();
    for (constructor, lambda_list) in boa_constructors(&form) {
        for variable in bare_aux_variables(lambda_list) {
            let Some(name) = atom_text(variable) else {
                continue;
            };
            if !form.declares_slot(name) {
                // An ordinary temporary, not a slot. Binding one to nil is
                // what a bare &aux is for outside a defstruct.
                continue;
            }
            let overrides_initform = form
                .slots
                .iter()
                .find(|slot| support::key(slot.name) == support::key(name))
                .is_some_and(|slot| slot.has_initform);
            found.push(DefstructBoaAuxUninitializedSlotItem {
                span: variable.span,
                structure: form.name.to_owned(),
                slot: name.to_owned(),
                constructor: constructor.to_owned(),
                overrides_initform,
            });
        }
    }

    if found.is_empty() || support::locate(tree, view.span).is_none_or(|site| site.quoted) {
        return;
    }
    found.sort_by_key(|item| item.span.start().get());
    violations.extend(found);
}

/// Collects every uninitialized BOA `&aux` slot in one file, with the number of
/// `defstruct` forms scanned as the denominator beside them.
pub fn build_defstruct_boa_aux_uninitialized_slot_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefstructBoaAuxUninitializedSlotItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("defstruct_form_count", json!(0))],
        ));
    }

    let mut defstruct_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let Some(view) = support::top_level_view(tree, index) else {
            continue;
        };
        let mut stack = vec![&view];
        while let Some(node) = stack.pop() {
            examine_defstruct_boa_aux_uninitialized_slot(
                tree,
                node,
                &mut defstruct_form_count,
                &mut violations,
            );
            stack.extend(node.children.iter());
        }
    }
    violations.sort_by_key(|item| item.span.start().get());

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("defstruct_form_count", json!(defstruct_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefstructBoaAuxUninitializedSlotItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defstruct_boa_aux_uninitialized_slot_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<DefstructBoaAuxUninitializedSlotItem> {
        report(input).findings
    }

    // ---- the defect, exactly as SBCL traps it ----

    #[test]
    fn flags_a_bare_aux_slot_that_overrides_an_initform() {
        let found =
            violations("(defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 999))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slot, "b");
        assert_eq!(found[0].structure, "rec");
        assert_eq!(found[0].constructor, "make-rec");
        assert!(found[0].overrides_initform);
    }

    #[test]
    fn flags_a_bare_aux_slot_that_had_no_initform_to_override() {
        let found = violations("(defstruct (rec (:constructor make-rec (a &aux b))) a b)");
        assert_eq!(found.len(), 1);
        assert!(!found[0].overrides_initform);
    }

    #[test]
    fn flags_a_parenthesised_aux_slot_with_no_value_form() {
        let found =
            violations("(defstruct (rec (:constructor make-rec (a &aux (b)))) (a 0) (b 9))");
        assert_eq!(
            found.len(),
            1,
            "(b) supplies no value, the same as a bare b"
        );
    }

    #[test]
    fn flags_each_bare_aux_slot_separately() {
        let found =
            violations("(defstruct (rec (:constructor make-rec (a &aux b c))) (a 0) (b 1) (c 2))");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].slot, "b");
        assert_eq!(found[1].slot, "c");
    }

    #[test]
    fn flags_the_offending_constructor_when_a_struct_declares_several() {
        let found = violations(
            "(defstruct (rec (:constructor make-rec (a b)) \
             (:constructor make-partial (a &aux b))) (a 0) (b 1))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].constructor, "make-partial");
    }

    // ---- the refuted premise: omitting a slot is correct and must be silent ----

    /// CLHS: "If a slot is not initialized in this way, it is initialized by
    /// evaluating slot-initform." Verified against SBCL 2.6.0 — see this
    /// module's header. This is the single most important negative case here:
    /// the rule this replaces would have fired on it.
    #[test]
    fn does_not_flag_a_slot_merely_absent_from_the_lambda_list() {
        let found = violations(
            "(defstruct (point (:constructor make-point (x y))) \
             (x 0) (y 0) (label \"none\") (scale 1.0))",
        );
        assert!(
            found.is_empty(),
            "an omitted slot still gets its :initform; that is not a defect"
        );
    }

    #[test]
    fn does_not_flag_an_aux_variable_with_a_value_form() {
        let found =
            violations("(defstruct (rec (:constructor make-rec (a &aux (b 5)))) (a 0) (b 999))");
        assert!(found.is_empty(), "&aux (b 5) initializes the slot to 5");
    }

    #[test]
    fn does_not_flag_an_aux_variable_that_names_no_slot() {
        let found =
            violations("(defstruct (rec (:constructor make-rec (a &aux tmp))) (a 0) (b 1))");
        assert!(
            found.is_empty(),
            "tmp is an ordinary temporary, not a slot of rec"
        );
    }

    #[test]
    fn does_not_flag_a_plain_boa_lambda_list() {
        assert!(violations("(defstruct (rec (:constructor make-rec (a b))) a b)").is_empty());
    }

    #[test]
    fn does_not_flag_an_optional_or_key_parameter() {
        let found =
            violations("(defstruct (rec (:constructor make-rec (a &optional b &key c))) a b c)");
        assert!(
            found.is_empty(),
            "&optional and &key slots default to nil by the lambda list, not by an override"
        );
    }

    #[test]
    fn does_not_flag_the_default_keyword_constructor() {
        assert!(violations("(defstruct rec a b)").is_empty());
        assert!(violations("(defstruct (rec (:constructor)) a b)").is_empty());
        assert!(violations("(defstruct (rec (:constructor make-rec)) a b)").is_empty());
    }

    #[test]
    fn does_not_flag_a_defstruct_written_inside_quoted_data() {
        let found = violations(
            "(setf template '(defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 9)))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defstruct_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defmacro m () `(defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 9)))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn folds_case_and_package_qualification_when_matching_the_slot() {
        let found = violations("(defstruct (rec (:constructor make-rec (a &aux B))) (a 0) (b 9))");
        assert_eq!(found.len(), 1, "B and b are the same slot");
    }

    #[test]
    fn the_denominator_counts_every_defstruct_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(defstruct plain a b)\n\
             (defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 9))",
        );
        assert_eq!(scanned.summary, vec![("defstruct_form_count", json!(2))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defstruct rec a)", Dialect::Clojure).expect("parse");
        let built = build_defstruct_boa_aux_uninitialized_slot_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_distinguishes_an_overridden_initform_from_a_missing_one() {
        let overriding =
            report("(defstruct (rec (:constructor make-rec (a &aux b))) (a 0) (b 999))");
        assert!(
            overriding.findings[0].message().contains("overridden"),
            "{}",
            overriding.findings[0].message()
        );

        let absent = report("(defstruct (rec (:constructor make-rec (a &aux b))) a b)");
        assert!(
            absent.findings[0].message().contains("no :initform either"),
            "{}",
            absent.findings[0].message()
        );
    }
}
