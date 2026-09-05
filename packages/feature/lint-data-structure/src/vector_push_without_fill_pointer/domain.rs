//! `vector-push-without-fill-pointer` detection: `vector-push` or
//! `vector-push-extend` on a vector this file made without a `:fill-pointer`.
//!
//! # The premise this rule corrects
//!
//! The proposed rule was "`vector-push-extend` on an array made without
//! `:adjustable t` / `:fill-pointer`". The `:adjustable` half is wrong, and it
//! is wrong in the direction that matters — it would have reported working
//! code. SBCL 2.6.0:
//!
//! ```text
//! === P7b: fill-pointer but NOT adjustable ===
//! (let ((v (make-array 3 :fill-pointer 0)))
//!   (dotimes (i 5) (vector-push-extend i v))
//!   v)                       => #(0 1 2 3 4)
//!   (adjustable-array-p v)   => T
//! ```
//!
//! Five elements pushed into a three-element vector, no `:adjustable t`
//! anywhere, no error. A vector with a fill pointer is *already* actually
//! adjustable as far as `vector-push-extend` is concerned, so requiring
//! `:adjustable` would have flagged that as a defect.
//!
//! What is actually required is the **fill pointer**, and CLHS says so for
//! both operators: `vector-push` and `vector-push-extend` take a
//! *vector-with-fill-pointer*. Without one there is no fill pointer to read,
//! increment, or store through, and the call cannot work at any size:
//!
//! ```text
//! === P7: vector-push-extend on a non-adjustable array ===
//! (let ((v (make-array 3)))
//!   (vector-push-extend 1 v))
//! ERROR: Value of V in (VECTOR-PUSH-EXTEND 1 V) is #(0 0 0),
//!        not a (AND VECTOR (NOT SIMPLE-ARRAY)).
//! ```
//!
//! # What SBCL already catches, and what it does not
//!
//! SBCL emitted a compile-time warning for the case above, because the vector
//! was a lexical whose type it could derive in the same function:
//!
//! ```text
//! caught WARNING: Derived type of V is (VALUES (SIMPLE-VECTOR 3) &OPTIONAL),
//!   conflicting with its asserted type (AND VECTOR (NOT SIMPLE-ARRAY)).
//! ```
//!
//! So for the tightest shape this rule is a second opinion. It is not a second
//! opinion once the vector is a `defparameter` global or crosses a function
//! boundary, where SBCL derives nothing and the first symptom is a runtime
//! error. It also reports without compiling, which is the point of a linter.
//!
//! # Deliberate limits, all in the direction of not reporting
//!
//! - **The vector must resolve to a `make-array` in this file**, by the same
//!   two paths `hash-table-literal-string-key-under-eql` uses — a lexical
//!   binding through the binding table, or a top-level
//!   `defparameter`/`defvar`. A vector from anywhere else is not chased.
//! - **A reassigned variable is not reported**, because the `make-array` this
//!   rule can see may not be the vector the push reaches.
//! - **`:fill-pointer nil` is treated as absent** and reported, because that is
//!   what it means; any other value, including a variable, counts as present.
//! - **`(make-array … :adjustable t)` with no fill pointer is still reported.**
//!   Adjustable is not the requirement, as the run above shows.
//!
//! # Scope
//!
//! Common Lisp only.

use std::cell::OnceCell;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::binding::{BindingTable, build_binding_table};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{self, keyword_argument};

/// The two operators that require a vector with a fill pointer.
pub const PUSH_OPERATORS: [&str; 2] = ["vector-push", "vector-push-extend"];

/// The `defvar`-family heads whose second operand is the initial value.
const GLOBAL_DEFINERS: [&str; 3] = ["defparameter", "defvar", "defconstant"];

#[derive(Debug, Clone)]
pub struct VectorPushWithoutFillPointerItem {
    /// The span of the push call.
    pub span: ByteSpan,
    pub operator: String,
    pub vector: String,
    /// Whether the `make-array` did supply `:adjustable`, which is the option
    /// people reach for and which does not help.
    pub adjustable: bool,
}

impl Finding for VectorPushWithoutFillPointerItem {
    fn kind(&self) -> &'static str {
        "vector-push-without-fill-pointer"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("vector={}", self.vector),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("vector", json!(self.vector)),
            ("adjustable", json!(self.adjustable)),
        ]
    }

    fn message(&self) -> String {
        let adjustable = if self.adjustable {
            " Supplying :adjustable t does not help: what these operators require is the fill \
             pointer, and a vector that has one is already adjustable enough for them."
        } else {
            ""
        };
        format!(
            "{} requires a vector with a fill pointer, but {} is made by a make-array in this \
             file that supplies no :fill-pointer, so every such call fails (SBCL: \"not a (AND \
             VECTOR (NOT SIMPLE-ARRAY))\"): add :fill-pointer 0 to the make-array.{}",
            self.operator, self.vector, adjustable
        )
    }
}

/// Whether a `make-array` call establishes a fill pointer.
///
/// `:fill-pointer nil` is explicitly *no* fill pointer, which is the default
/// and is reported; anything else — `0`, `t`, a variable — counts as present,
/// because a rule that cannot read the value should not assume it is nil.
fn has_fill_pointer(make_call: &ExpressionView) -> bool {
    keyword_argument(make_call, 2, "fill-pointer")
        .is_some_and(|value| atom_text(value).is_none_or(|text| !text.eq_ignore_ascii_case("nil")))
}

/// Whether a `make-array` call supplies a non-nil `:adjustable`.
fn is_adjustable(make_call: &ExpressionView) -> bool {
    keyword_argument(make_call, 2, "adjustable")
        .is_some_and(|value| atom_text(value).is_none_or(|text| !text.eq_ignore_ascii_case("nil")))
}

/// The `make-array` a reference was initialized with, by the same two paths
/// `hash-table-literal-string-key-under-eql` uses and for the same reason: the
/// binding table does not build a binding for a top-level
/// `defparameter`/`defvar`, so globals need a by-name fallback. That gap lives
/// in `packages/core/semantics` and is reported rather than patched here.
fn array_construction(
    tree: &SyntaxTree,
    bindings: &BindingTable,
    reference: &ExpressionView,
) -> Option<ExpressionView> {
    if let Some(id) = NodeKey::of(reference).and_then(|node| bindings.resolve(node)) {
        let binding = bindings.binding(id);
        if !binding.assignments().is_empty() {
            return None;
        }
        let init = support::view_at_span(tree, binding.init_form()?)?;
        return list_head(&init)
            .is_some_and(|head| symbol_is(head, "make-array"))
            .then_some(init);
    }
    global_array_construction(tree, atom_text(reference)?)
}

/// The `make-array` a top-level `defparameter`/`defvar` gives `name`, if
/// nothing in the file assigns `name` afterwards.
fn global_array_construction(tree: &SyntaxTree, name: &str) -> Option<ExpressionView> {
    let wanted = support::key(name);
    let definition = support::top_level_heads(tree)
        .filter(|top| GLOBAL_DEFINERS.iter().any(|head| symbol_is(top.head, head)))
        .filter_map(|top| support::top_level_view(tree, top.index))
        .find(|view| {
            view.children
                .get(1)
                .and_then(atom_text)
                .is_some_and(|bound| support::key(bound) == wanted)
        })?;
    let init = definition.children.get(2)?;
    if !list_head(init).is_some_and(|head| symbol_is(head, "make-array")) {
        return None;
    }
    if assigns_global(tree, &wanted) {
        return None;
    }
    Some(init.clone())
}

/// Whether any `(setf name …)` or `(setq name …)` in the file writes `name`.
///
/// The walk borrows, for the reason spelled out on the same function in
/// `hash_table_literal_string_key_under_eql::domain`: `ExpressionView`'s
/// `Clone` is a deep recursive subtree copy, and cloning children into the
/// stack copies the whole file once per call. Measured at 7989258
/// ns/invocation before this was written by reference.
fn assigns_global(tree: &SyntaxTree, wanted: &str) -> bool {
    // Cheap byte scan first: no `(setf name …)` text means no assignment, and
    // the walk below need not run at all.
    if !support::might_assign(tree.source(), wanted) {
        return false;
    }
    (0..tree.root_children().len())
        .filter_map(|index| support::top_level_view(tree, index))
        .any(|top| {
            let mut stack: Vec<&ExpressionView> = vec![&top];
            while let Some(node) = stack.pop() {
                let writes = list_head(node)
                    .is_some_and(|head| symbol_is(head, "setf") || symbol_is(head, "setq"))
                    && node
                        .children
                        .iter()
                        .skip(1)
                        .step_by(2)
                        .filter_map(atom_text)
                        .any(|place| support::key(place) == wanted);
                if writes {
                    return true;
                }
                stack.extend(node.children.iter());
            }
            false
        })
}

///
/// The head match and the "is the vector a plain name" check are local reads on
/// this node; only once both hold does anything ask for the binding table,
/// which is a whole-file semantic build.
///
/// `bindings` is a **closure** to make that ordering real rather than merely
/// intended. Taking `&BindingTable` by value has the caller evaluate
/// `context.binding_table()` at the call site, before this function runs, so
/// every `vector-push` in the file pays for the build whether or not its vector
/// is even a name. Measured, that mistake cost 9431602 ns/invocation against
/// 164 ns for the cheapest rule in this package.
pub fn examine_vector_push_without_fill_pointer<'a>(
    tree: &SyntaxTree,
    bindings: impl FnOnce() -> &'a BindingTable,
    view: &ExpressionView,
    push_form_count: &mut usize,
    violations: &mut Vec<VectorPushWithoutFillPointerItem>,
) {
    if !is_paren_list(view) {
        return;
    }
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = PUSH_OPERATORS
        .iter()
        .find(|name| symbol_is(head, name))
        .copied()
    else {
        return;
    };
    *push_form_count += 1;

    // `(vector-push new-element vector)`: the vector is the second operand.
    let Some(vector_view) = view.children.get(2) else {
        return;
    };
    let Some(vector) = atom_text(vector_view) else {
        return;
    };
    // Past every cheap gate: only now is the semantic table worth building.
    let Some(make_call) = array_construction(tree, bindings(), vector_view) else {
        return;
    };
    if has_fill_pointer(&make_call) {
        return;
    }
    if support::locate(tree, view.span).is_none_or(|site| site.quoted) {
        return;
    }
    violations.push(VectorPushWithoutFillPointerItem {
        span: view.span,
        operator: operator.to_owned(),
        vector: vector.to_owned(),
        adjustable: is_adjustable(&make_call),
    });
}

/// Collects every fill-pointer-less push in one file, with the number of
/// `vector-push`/`vector-push-extend` calls scanned as the denominator.
pub fn build_vector_push_without_fill_pointer_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<VectorPushWithoutFillPointerItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("push_form_count", json!(0))],
        ));
    }

    // Lazy here too, so a file with no resolvable pushed vector never pays for
    // the semantic build — the same property the lint rule has.
    let bindings: OnceCell<BindingTable> = OnceCell::new();
    let mut push_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let Some(view) = support::top_level_view(tree, index) else {
            continue;
        };
        let mut stack = vec![&view];
        while let Some(node) = stack.pop() {
            examine_vector_push_without_fill_pointer(
                tree,
                || bindings.get_or_init(|| build_binding_table(dialect, tree, tree.source())),
                node,
                &mut push_form_count,
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
        vec![("push_form_count", json!(push_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<VectorPushWithoutFillPointerItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_vector_push_without_fill_pointer_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<VectorPushWithoutFillPointerItem> {
        report(input).findings
    }

    // ---- the defect ----

    #[test]
    fn flags_a_push_onto_a_plain_make_array() {
        let found = violations("(let ((v (make-array 3))) (vector-push-extend 1 v))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].vector, "v");
        assert_eq!(found[0].operator, "vector-push-extend");
        assert!(!found[0].adjustable);
    }

    #[test]
    fn flags_plain_vector_push_too() {
        let found = violations("(let ((v (make-array 3))) (vector-push 1 v))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].operator, "vector-push");
    }

    /// The corrected premise: `:adjustable t` is not what these operators
    /// need, so supplying it and no fill pointer is still broken.
    #[test]
    fn flags_an_adjustable_array_with_no_fill_pointer() {
        let found = violations("(let ((v (make-array 3 :adjustable t))) (vector-push-extend 1 v))");
        assert_eq!(found.len(), 1);
        assert!(found[0].adjustable);
        assert!(
            found[0].message().contains(":adjustable t does not help"),
            "{}",
            found[0].message()
        );
    }

    #[test]
    fn flags_an_explicit_nil_fill_pointer() {
        let found =
            violations("(let ((v (make-array 3 :fill-pointer nil))) (vector-push-extend 1 v))");
        assert_eq!(found.len(), 1, ":fill-pointer nil is no fill pointer");
    }

    #[test]
    fn flags_a_defparameter_vector() {
        let found = violations(
            "(defparameter *buf* (make-array 8))\n(defun add (x) (vector-push-extend x *buf*))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].vector, "*buf*");
    }

    // ---- correct code, which must stay silent ----

    /// Verified against SBCL 2.6.0: a fill pointer with no `:adjustable`
    /// accepts five pushes into a three-element vector. See the module header.
    #[test]
    fn does_not_flag_a_fill_pointer_without_adjustable() {
        let found =
            violations("(let ((v (make-array 3 :fill-pointer 0))) (vector-push-extend 1 v))");
        assert!(
            found.is_empty(),
            "a fill pointer alone is what these operators require"
        );
    }

    #[test]
    fn does_not_flag_the_fully_specified_form() {
        let found = violations(
            "(let ((v (make-array 3 :fill-pointer 0 :adjustable t))) (vector-push-extend 1 v))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_fill_pointer_this_rule_cannot_evaluate() {
        let found =
            violations("(let ((v (make-array 3 :fill-pointer start))) (vector-push-extend 1 v))");
        assert!(found.is_empty(), "an unreadable value is not an absent one");
    }

    #[test]
    fn does_not_flag_a_vector_this_file_does_not_construct() {
        assert!(violations("(defun add (v x) (vector-push-extend x v))").is_empty());
        assert!(violations("(vector-push-extend 1 (buffer-of obj))").is_empty());
    }

    #[test]
    fn does_not_flag_a_vector_bound_to_something_other_than_make_array() {
        assert!(violations("(let ((v (make-buffer))) (vector-push-extend 1 v))").is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_that_is_reassigned() {
        let found = violations(
            "(let ((v (make-array 3))) (setf v (make-array 3 :fill-pointer 0)) \
             (vector-push-extend 1 v))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_global_the_file_reassigns() {
        let found = violations(
            "(defparameter *buf* (make-array 8))\n\
             (defun reset () (setf *buf* (make-array 8 :fill-pointer 0)))\n\
             (defun add (x) (vector-push-extend x *buf*))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_push_written_inside_quoted_data() {
        let found = violations("(let ((v (make-array 3))) (setf form '(vector-push-extend 1 v)))");
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_push_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(let ((good (make-array 3 :fill-pointer 0)) (bad (make-array 3))) \
             (vector-push-extend 1 good) (vector-push-extend 2 bad) (vector-push 3 good))",
        );
        assert_eq!(scanned.summary, vec![("push_form_count", json!(3))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(vector-push 1 v)", Dialect::Clojure).expect("parse");
        let built = build_vector_push_without_fill_pointer_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_names_the_repair() {
        let built = report("(let ((v (make-array 3))) (vector-push-extend 1 v))");
        assert!(
            built.findings[0].message().contains(":fill-pointer 0"),
            "{}",
            built.findings[0].message()
        );
    }
}
