//! `defclass-required-slot-no-initform-or-initarg` detection: a slot with
//! neither `:initform` nor `:initarg`, whose value a method in the same file
//! reads.
//!
//! A slot declared with neither option starts *unbound*. That is legal, and for
//! a slot nothing ever reads it is even sensible. What makes it a defect is a
//! read: `make-instance` has no way to fill the slot — there is no `:initarg`
//! to pass and no `:initform` to evaluate — so the first read signals
//! `unbound-slot` on a freshly made instance.
//!
//! ## What "reads it" means here, and what it does not
//!
//! Proving that a read happens *before* any write would need whole-program
//! ordering, which a per-file lint pass does not have. What this rule proves is
//! weaker and stated as such: the slot cannot be given a value at construction
//! time, and some method in the file reads it. Three things narrow that to the
//! cases worth reporting:
//!
//! - **A read, not a write.** The place of a `setf`/`psetf` is excluded, so the
//!   `(setf (slot-value o 'x) v)` that fills the slot is not itself the
//!   evidence that reading it is unsafe.
//! - **Only methods.** A read from a `defun` is not counted: this rule is about
//!   the instance protocol, and `defmethod` bodies are where an instance
//!   arrives without having been through the reader's own code.
//! - **An initializer silences the whole class.** A file with an
//!   `initialize-instance`, `shared-initialize` or `reinitialize-instance`
//!   method has somewhere for the assignment to live, and this rule stops
//!   guessing.
//!
//! Deliberately *not* a duplicate of `lint-convention`'s `defclass-slot-option`,
//! which validates the spelling of the options a slot *has* and never looks at
//! the ones it lacks.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, for_each_subview, list_head, symbol_in, symbol_is,
};
use serde_json::{Value, json};

use crate::support::{self, SlotSpec};

/// The methods whose whole job is to give slots values. One of them anywhere in
/// the file means the assignment this rule cannot find has a place to be.
const INITIALIZERS: [&str; 3] = [
    "initialize-instance",
    "shared-initialize",
    "reinitialize-instance",
];

#[derive(Debug, Clone)]
pub struct DefclassRequiredSlotNoInitformOrInitargItem {
    /// The span of the slot spec, not of the whole `defclass`.
    pub span: ByteSpan,
    pub slot: String,
    pub class: String,
    /// The method whose body reads the slot.
    pub read_by: String,
}

impl Finding for DefclassRequiredSlotNoInitformOrInitargItem {
    fn kind(&self) -> &'static str {
        "defclass-required-slot-no-initform-or-initarg"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("slot={}", self.slot),
            format!("class={}", self.class),
            format!("read_by={}", self.read_by),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("slot", json!(self.slot)),
            ("class", json!(self.class)),
            ("read_by", json!(self.read_by)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "slot {} of {} has neither :initform nor :initarg, so make-instance leaves it \
             unbound, and the method {} reads it: the first read signals unbound-slot",
            self.slot, self.class, self.read_by
        )
    }
}

/// Whether `view` is a read of `slot` — either `(slot-value o 'slot)` or a call
/// to one of the slot's own readers.
fn is_slot_read(view: &ExpressionView, slot: &str, readers: &[&str]) -> bool {
    if support::calls_head(view, "slot-value") {
        return view
            .children
            .get(2)
            .and_then(support::quoted_symbol)
            .is_some_and(|named| symbol_is(named, slot));
    }
    list_head(view).is_some_and(|head| symbol_in(head, readers))
}

/// Whether `view` binds `slot` with `with-slots`/`with-accessors`, which makes
/// every mention of the bound name inside a read of it.
fn binds_slot(view: &ExpressionView, slot: &str) -> bool {
    if !(support::calls_head(view, "with-slots") || support::calls_head(view, "with-accessors")) {
        return false;
    }
    view.children.get(1).is_some_and(|bindings| {
        bindings.children.iter().any(|binding| {
            let named = atom_text(binding)
                .or_else(|| binding.children.last().and_then(atom_text))
                .unwrap_or_default();
            symbol_is(named, slot)
        })
    })
}

/// Whether anything under `view` reads `slot`.
///
/// Two positions are excluded. Quoted data is not code, so a `(slot-value o
/// 'x)` written inside `'(…)` reads nothing. And the place of a `setf` is
/// written, not read — though its *subforms* still count: `(setf (slot-value
/// (owner o) 'x) v)` reads nothing about `x`, but `(setf (foo (slot-value o
/// 'x)) v)` reads it.
fn reads_slot(view: &ExpressionView, slot: &str, readers: &[&str]) -> bool {
    if support::is_quoted_here(view) {
        return false;
    }
    if is_slot_read(view, slot, readers) || binds_slot(view, slot) {
        return true;
    }
    if support::calls_head(view, "setf") || support::calls_head(view, "psetf") {
        return view
            .children
            .iter()
            .enumerate()
            .skip(1)
            .any(|(index, child)| {
                if index % 2 == 1 && is_slot_read(child, slot, readers) {
                    child
                        .children
                        .iter()
                        .any(|inner| reads_slot(inner, slot, readers))
                } else {
                    reads_slot(child, slot, readers)
                }
            });
    }
    view.children
        .iter()
        .any(|child| reads_slot(child, slot, readers))
}

/// Every top-level `defmethod` in the file, with the name it defines.
///
/// Materialized once per `defclass` that has at least one candidate slot, and
/// shared across that class's slots — a class with six unbound slots walks the
/// file's methods once, not six times.
fn method_bodies(tree: &SyntaxTree) -> Vec<(String, ExpressionView)> {
    support::top_level_forms(tree)
        .filter(|form| symbol_is(form.head, "defmethod"))
        .filter_map(|form| support::top_level_view(tree, form.index))
        .map(|view| {
            let name =
                support::definition_name(&view).unwrap_or_else(|| "an unnamed method".to_owned());
            (name, view)
        })
        .collect()
}

/// The first method that reads `slot`, in file order.
fn read_by<'a>(methods: &'a [(String, ExpressionView)], slot: &SlotSpec<'_>) -> Option<&'a str> {
    let readers = slot.reader_names();
    methods
        .iter()
        .find(|(_, view)| reads_slot(view, slot.name, &readers))
        .map(|(name, _)| name.as_str())
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_defclass_required_slot_no_initform_or_initarg(
    tree: &SyntaxTree,
    view: &ExpressionView,
    slot_count: &mut usize,
    violations: &mut Vec<DefclassRequiredSlotNoInitformOrInitargItem>,
) {
    let Some(class) = support::class_form(view) else {
        return;
    };
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *slot_count += class.slots.len();

    let candidates: Vec<&SlotSpec<'_>> = class
        .slots
        .iter()
        .filter(|slot| !slot.has_option(":initform") && !slot.has_option(":initarg"))
        .collect();
    if candidates.is_empty() {
        return;
    }

    let methods = method_bodies(tree);
    if methods
        .iter()
        .any(|(name, _)| symbol_in(name, &INITIALIZERS))
    {
        return;
    }

    for slot in candidates {
        let Some(reader) = read_by(&methods, slot) else {
            continue;
        };
        violations.push(DefclassRequiredSlotNoInitformOrInitargItem {
            span: slot.span,
            slot: slot.name.to_owned(),
            class: class.name.to_owned(),
            read_by: reader.to_owned(),
        });
    }
}

/// Collects every unbound-on-read slot in one file, with the number of slots
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: `:initform`, `:initarg` and `unbound-slot` are CLOS's.
pub fn build_defclass_required_slot_no_initform_or_initarg_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefclassRequiredSlotNoInitformOrInitargItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("slot_count", json!(0))],
        ));
    }

    let mut slot_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_defclass_required_slot_no_initform_or_initarg(
                tree,
                subview,
                &mut slot_count,
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
        vec![("slot_count", json!(slot_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefclassRequiredSlotNoInitformOrInitargItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defclass_required_slot_no_initform_or_initarg_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build required slot report")
    }

    fn violations(input: &str) -> Vec<DefclassRequiredSlotNoInitformOrInitargItem> {
        report(input).findings
    }

    #[test]
    fn flags_an_optionless_slot_a_method_reads() {
        let found = violations(
            "(defclass circle () ((radius)))\n(defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slot, "radius");
        assert_eq!(found[0].class, "circle");
        assert_eq!(found[0].read_by, "area");
    }

    #[test]
    fn flags_a_bare_symbol_slot_too() {
        let found = violations(
            "(defclass circle () (radius))\n(defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_a_slot_read_through_its_own_reader() {
        let found = violations(
            "(defclass circle () ((radius :reader radius-of)))\n\
             (defmethod area ((c circle)) (* pi (radius-of c)))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].read_by, "area");
    }

    #[test]
    fn flags_a_slot_bound_by_with_slots() {
        let found = violations(
            "(defclass circle () ((radius)))\n\
             (defmethod area ((c circle)) (with-slots (radius) c radius))",
        );
        assert_eq!(found.len(), 1);
    }

    /// The near miss: the slot can be filled, so a read is fine.
    #[test]
    fn does_not_flag_a_slot_with_an_initarg() {
        let found = violations(
            "(defclass circle () ((radius :initarg :radius)))\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_slot_with_an_initform() {
        let found = violations(
            "(defclass circle () ((radius :initform 1)))\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_an_optionless_slot_nothing_reads() {
        let found = violations(
            "(defclass circle () ((cache)))\n(defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_slot_only_a_setf_place_mentions() {
        let found = violations(
            "(defclass circle () ((cache)))\n\
             (defmethod warm ((c circle)) (setf (slot-value c 'cache) 1))",
        );
        assert!(found.is_empty(), "the place of a setf is written, not read");
    }

    #[test]
    fn does_not_flag_a_read_from_a_plain_function() {
        let found =
            violations("(defclass circle () ((radius)))\n(defun area (c) (slot-value c 'radius))");
        assert!(found.is_empty());
    }

    #[test]
    fn an_initialize_instance_method_silences_the_class() {
        let found = violations(
            "(defclass circle () ((radius)))\n\
             (defmethod initialize-instance :after ((c circle) &key) (setf (slot-value c 'radius) 1))\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_defclass() {
        let found = violations(
            "(setf template '(defclass circle () ((radius))))\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defclass_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defmacro m () `(defclass circle () ((radius))))\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defclass_written_inside_a_string() {
        let found = violations(
            "(defparameter *doc* \"(defclass circle () ((radius)))\")\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn a_quoted_read_does_not_count() {
        let found = violations(
            "(defclass circle () ((radius)))\n(defmethod area ((c circle)) '(slot-value c 'radius))",
        );
        assert!(
            found.is_empty(),
            "a slot-value form inside quoted data reads nothing"
        );
    }

    #[test]
    fn the_denominator_counts_every_slot_scanned() {
        let built = report(
            "(defclass circle () ((radius) (centre :initform 0)))\n\
             (defmethod area ((c circle)) (slot-value c 'radius))",
        );
        assert_eq!(built.summary, vec![("slot_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defclass circle () ((radius)))", Dialect::Clojure)
                .expect("parse");
        let built = build_defclass_required_slot_no_initform_or_initarg_report(
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
        let built = report(
            "(defclass circle ()\n  ((radius)))\n(defmethod area ((c circle)) (slot-value c 'radius))",
        );
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "defclass-required-slot-no-initform-or-initarg"
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "slot=radius".to_owned(),
                "class=circle".to_owned(),
                "read_by=area".to_owned(),
            ]
        );
        assert!(finding.message().contains("unbound-slot"));
    }
}
