//! `slot-value-bypasses-accessor` detection: `(slot-value o 'x)` in a file that
//! also declares a `:reader` or `:accessor` for slot `x`.
//!
//! An accessor is a generic function. Reaching past it with `slot-value` skips
//! everything the accessor's method combination adds — an `:around` that
//! memoizes, a `:before` that faults the value in, a subclass method that
//! computes the slot instead of storing it — and reads the raw storage instead.
//! The result is right until one of those is added, and then it is silently
//! stale.
//!
//! Three things keep this from firing on the code that is *supposed* to use
//! `slot-value`:
//!
//! - **Read position only.** `(setf (slot-value b 'contents) items)` is a
//!   *write*, and this rule has nothing to say about it. Suggesting
//!   `(setf (box-contents b) items)` would be wrong outright when `contents`
//!   carries only a `:reader` — there is no such writer function, and the
//!   advice would not compile. It would also fire on the canonical
//!   `(setf (slot-value …) …)` in an `initialize-instance :after` method, which
//!   is how a computed slot is supposed to be filled. See
//!   [`support::is_write_position`] for which operand of which macro counts.
//! - **The slot must have a reader.** A slot declared with only a `:writer`, or
//!   with no accessor at all, has no accessor to bypass, and `slot-value` is
//!   the only way to read it.
//! - **The accessor's own methods are exempt.** Inside `(defmethod radius …)`
//!   — a hand-written method on the accessor generic itself — `slot-value` is
//!   how the value is finally reached, and going through the accessor there
//!   would recurse.
//!
//! Only a constant slot name is read: `(slot-value o name)` names a slot chosen
//! at run time, and no static rule can say which.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, symbol_is, unqualified};
use serde_json::{Value, json};

use crate::support;

#[derive(Debug, Clone)]
pub struct SlotValueBypassesAccessorItem {
    /// The span of the whole `(slot-value …)` form.
    pub span: ByteSpan,
    pub slot: String,
    /// The reader or accessor this read goes around.
    pub accessor: String,
    /// The class that declares it, so a reader knows where to look.
    pub class: String,
}

impl Finding for SlotValueBypassesAccessorItem {
    fn kind(&self) -> &'static str {
        "slot-value-bypasses-accessor"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("slot={}", self.slot),
            format!("accessor={}", self.accessor),
            format!("class={}", self.class),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("slot", json!(self.slot)),
            ("accessor", json!(self.accessor)),
            ("class", json!(self.class)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "slot-value reads {} directly although {} declares the accessor {}: this skips \
             whatever that generic's method combination adds",
            self.slot, self.class, self.accessor
        )
    }
}

/// The first same-file class declaring a reader for `slot`, as `(accessor,
/// class)`.
///
/// Scanned in file order so the answer is the same on every run; a slot of that
/// name declared by two classes reports the first, which is enough to say the
/// read has an accessor available.
fn declared_reader(tree: &SyntaxTree, slot: &str) -> Option<(String, String)> {
    for form in support::top_level_forms(tree) {
        if !symbol_is(form.head, "defclass") {
            continue;
        }
        let Some(view) = support::top_level_view(tree, form.index) else {
            continue;
        };
        let Some(class) = support::class_form(&view) else {
            continue;
        };
        for declared in &class.slots {
            if !symbol_is(declared.name, slot) {
                continue;
            }
            if let Some(reader) = declared.reader_names().first() {
                return Some(((*reader).to_owned(), class.name.to_owned()));
            }
        }
    }
    None
}

/// Whether the top-level form at `index` is a method or function *on* the
/// accessor generic itself, where `slot-value` is how the value is finally
/// reached.
///
/// Covers the `(setf accessor)` writer half too, which is the other place the
/// raw slot has to be touched.
fn defines_the_accessor(tree: &SyntaxTree, index: usize, accessor: &str) -> bool {
    let Some(view) = support::top_level_view(tree, index) else {
        return false;
    };
    if !(support::calls_head(&view, "defmethod") || support::calls_head(&view, "defun")) {
        return false;
    }
    let Some(name) = support::definition_name(&view) else {
        return false;
    };
    let accessor = unqualified(accessor).to_ascii_lowercase();
    name == accessor || name == format!("(setf {accessor})")
}

pub fn examine_slot_value_bypasses_accessor(
    tree: &SyntaxTree,
    view: &ExpressionView,
    slot_value_form_count: &mut usize,
    violations: &mut Vec<SlotValueBypassesAccessorItem>,
) {
    if !support::calls_head(view, "slot-value") {
        return;
    }
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    // Not `top_level`: a `slot-value` call is never a top-level definition. All
    // that matters is that the reader would evaluate it.
    if site.quoted {
        return;
    }
    *slot_value_form_count += 1;

    // The denominator stays "every `slot-value` form the reader evaluates",
    // which is what it says; the write-position skip is a filter on reporting,
    // applied after it.
    if support::is_write_position(tree, view.span) != Some(false) {
        return;
    }

    let Some(slot) = view.children.get(2).and_then(support::quoted_symbol) else {
        return;
    };
    let Some((accessor, class)) = declared_reader(tree, slot) else {
        return;
    };
    if defines_the_accessor(tree, site.top_level_index, &accessor) {
        return;
    }

    violations.push(SlotValueBypassesAccessorItem {
        span: view.span,
        slot: slot.to_owned(),
        accessor,
        class,
    });
}

/// Collects every accessor-bypassing `slot-value` read in one file, with the
/// number of `slot-value` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_slot_value_bypasses_accessor_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SlotValueBypassesAccessorItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("slot_value_form_count", json!(0))],
        ));
    }

    let mut slot_value_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_slot_value_bypasses_accessor(
                tree,
                subview,
                &mut slot_value_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("slot_value_form_count", json!(slot_value_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SlotValueBypassesAccessorItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_slot_value_bypasses_accessor_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build slot value bypasses accessor report")
    }

    fn violations(input: &str) -> Vec<SlotValueBypassesAccessorItem> {
        report(input).findings
    }

    const WITH_ACCESSOR: &str = "(defclass circle () ((radius :accessor radius-of)))\n";
    const WITH_WRITER_ONLY: &str = "(defclass circle () ((radius :writer set-radius)))\n";

    #[test]
    fn flags_a_slot_value_read_of_a_slot_that_has_an_accessor() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun area (c) (* pi (slot-value c 'radius)))"
        ));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slot, "radius");
        assert_eq!(found[0].accessor, "radius-of");
        assert_eq!(found[0].class, "circle");
    }

    #[test]
    fn flags_the_spelled_out_quote_form_too() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun area (c) (slot-value c (quote radius)))"
        ));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_a_read_of_a_reader_only_slot() {
        let found = violations(
            "(defclass circle () ((radius :reader radius-of)))\n(defun area (c) (slot-value c 'radius))",
        );
        assert_eq!(found.len(), 1);
    }

    /// The near miss: the slot has no way to be read but `slot-value`.
    #[test]
    fn does_not_flag_a_slot_with_no_reader() {
        let found = violations(&format!(
            "{WITH_WRITER_ONLY}(defun area (c) (slot-value c 'radius))"
        ));
        assert!(found.is_empty());
    }

    /// The reported false positive: `contents` has a `:reader` and no writer at
    /// all, so the `(setf (box-contents b) …)` this rule used to recommend
    /// names a function that does not exist.
    #[test]
    fn does_not_flag_a_write_through_a_reader_only_slot() {
        let found = violations(
            "(defclass box () ((contents :reader box-contents)))\n\
             (defun fill-box (b items) (setf (slot-value b 'contents) items))",
        );
        assert!(
            found.is_empty(),
            "a setf place is a write, and there is no writer to route it through"
        );
    }

    /// The canonical place a `slot-value` write belongs: filling a computed
    /// slot after the standard initialization has run.
    #[test]
    fn does_not_flag_the_write_in_an_initialize_instance_after_method() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defmethod initialize-instance :after ((c circle) &key)\n  \
             (setf (slot-value c 'radius) 1))"
        ));
        assert!(found.is_empty());
    }

    /// `incf` and friends read the place as well as writing it, but they read
    /// it through the same `(setf …)` expansion, so a slot with no writer is no
    /// more reachable there than in a bare `setf`.
    #[test]
    fn does_not_flag_a_read_modify_write_place() {
        for form in [
            "(incf (slot-value c 'radius))",
            "(decf (slot-value c 'radius) 2)",
            "(push x (slot-value c 'radius))",
            "(pop (slot-value c 'radius))",
            "(rotatef (slot-value c 'radius) other)",
        ] {
            let found = violations(&format!("{WITH_ACCESSOR}(defun f (c x) {form})"));
            assert!(found.is_empty(), "for {form}");
        }
    }

    /// The other half of the write skip: a `slot-value` on the *value* side of
    /// a `setf` is an ordinary read and is still reported.
    #[test]
    fn still_flags_a_read_on_the_value_side_of_a_setf() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun f (c) (setf *cache* (slot-value c 'radius)))"
        ));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slot, "radius");
    }

    /// Only the immediate parent decides. The inner call is how the object the
    /// outer place writes into is found, which is a read.
    #[test]
    fn still_flags_the_object_read_inside_a_nested_place() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun f (c v) (setf (other (slot-value c 'radius)) v))"
        ));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_written_slot_value_form_still_counts_toward_the_denominator() {
        let built = report(&format!(
            "{WITH_ACCESSOR}(defun f (c v) (setf (slot-value c 'radius) v))"
        ));
        assert_eq!(
            built.summary,
            vec![("slot_value_form_count", json!(1))],
            "the denominator is every slot-value form scanned, reported or not"
        );
        assert!(built.findings.is_empty());
    }

    #[test]
    fn does_not_flag_a_slot_no_defclass_in_the_file_declares() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun area (c) (slot-value c 'centre))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_slot_name_computed_at_run_time() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun area (c n) (slot-value c n))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_the_accessors_own_method() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defmethod radius-of ((c circle)) (slot-value c 'radius))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_the_accessors_own_setf_method() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defmethod (setf radius-of) (v (c circle)) (setf (slot-value c 'radius) v))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn flags_a_read_inside_a_method_on_a_different_generic() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defmethod draw ((c circle)) (slot-value c 'radius))"
        ));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn does_not_flag_a_quoted_slot_value_form() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defun area (c) '(slot-value c 'radius))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_slot_value_form_inside_an_unescaped_quasiquote() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defmacro reader (c) `(slot-value ,c 'radius))"
        ));
        assert!(
            found.is_empty(),
            "the (slot-value …) list is data; only the ,c inside it is code"
        );
    }

    #[test]
    fn does_not_flag_a_slot_value_form_written_inside_a_string() {
        let found = violations(&format!(
            "{WITH_ACCESSOR}(defparameter *doc* \"(slot-value c 'radius)\")"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_slot_value_form_scanned() {
        let built = report(&format!(
            "{WITH_ACCESSOR}(defun area (c) (+ (slot-value c 'radius) (slot-value c 'centre)))"
        ));
        assert_eq!(built.summary, vec![("slot_value_form_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(slot-value c 'radius)", Dialect::Clojure)
            .expect("parse");
        let built = build_slot_value_bypasses_accessor_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let built = report(&format!(
            "{WITH_ACCESSOR}(defun area (c)\n  (slot-value c 'radius))"
        ));
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 3);
        assert_eq!(finding.kind(), "slot-value-bypasses-accessor");
        assert_eq!(
            finding.text_columns(),
            vec![
                "slot=radius".to_owned(),
                "accessor=radius-of".to_owned(),
                "class=circle".to_owned(),
            ]
        );
    }
}
