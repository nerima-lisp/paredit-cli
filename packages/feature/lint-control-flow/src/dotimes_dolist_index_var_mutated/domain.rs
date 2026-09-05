//! Common Lisp mutated-iteration-variable detection: the `var` of a
//! `(dotimes (var count) …)` or `(dolist (var list) …)` assigned inside the
//! body.
//!
//! The two heads earn a finding for two different reasons, and the message
//! says which:
//!
//! - **`dotimes`.** CLHS `dotimes`: "It is implementation-dependent whether
//!   dotimes establishes a new binding of var on each iteration or whether it
//!   establishes a binding for var once at the beginning and then assigns it
//!   on any subsequent iterations." An implementation that expands to
//!   `(do ((var 0 (1+ var))) …)` — the classic expansion — lets an assignment
//!   change how many iterations run; one that copies an internal counter into
//!   a fresh binding does not. So the *effect on the loop* is
//!   implementation-dependent, which is exactly what a portable program may
//!   not depend on.
//!
//! - **`dolist`.** The iteration is driven by the list, never by `var`, so an
//!   assignment cannot change it — and the next iteration overwrites `var`
//!   from the list either way. That makes assigning it well-defined, so this
//!   rule reports it **only when nothing reads `var` again afterwards in the
//!   body**, where the assignment is simply discarded. Using the loop variable
//!   as a scratch variable — `(dolist (x l) (setq x (normalize x)) (use x))` —
//!   is legal, portable, and not reported.
//!
//! # What stops a finding
//!
//! - A `let`, `lambda`, `destructuring-bind`, nested `dolist`/`dotimes`/`do`,
//!   `multiple-value-bind`, `symbol-macrolet`, `with-slots`, `flet` or
//!   `labels` that rebinds the same name: the assignment under it is to a
//!   different variable, and the walk does not descend into such a form.
//! - A place that is not the bare symbol. `(setf (car x) 1)` mutates the
//!   object `x` points at, which is an ordinary and correct thing to do.
//! - Quoted data.
//!
//! # Relationship to `dotimes-bound-mutation-has-no-effect`
//!
//! `paredit-feature-lint-iteration-flow`'s
//! `dotimes-bound-mutation-has-no-effect` is about the *count* variable —
//! `(dotimes (i n) … (setq n 0))` — which is child 2 of the spec. This rule is
//! about child 1, the iteration variable. The two are structurally disjoint
//! and can both fire on a form that mutates both.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{
    for_each_subview, is_paren_list, list_head, symbol_in, symbol_is,
};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview_where, is_unevaluated_at, normalized_symbol, plain_name,
};

/// Which of the two iteration macros a finding is about, since the complaint
/// differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterationForm {
    Dotimes,
    Dolist,
}

impl IterationForm {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Dotimes => "dotimes",
            Self::Dolist => "dolist",
        }
    }

    fn read(head: &str) -> Option<Self> {
        if symbol_is(head, "dotimes") {
            return Some(Self::Dotimes);
        }
        symbol_is(head, "dolist").then_some(Self::Dolist)
    }
}

#[derive(Debug, Clone)]
pub struct DotimesDolistIndexVarMutatedItem {
    /// The span of the assignment form, not of the whole loop: that is what a
    /// reader has to look at.
    pub span: ByteSpan,
    /// The iteration variable, normalized.
    pub variable: String,
    /// Which macro binds it.
    pub form: IterationForm,
}

impl Finding for DotimesDolistIndexVarMutatedItem {
    fn kind(&self) -> &'static str {
        "dotimes-dolist-index-var-mutated"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("variable={}", self.variable),
            format!("form={}", self.form.as_str()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("variable", json!(self.variable)),
            ("form", json!(self.form.as_str())),
        ]
    }

    fn message(&self) -> String {
        message_for(&self.variable, self.form)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(variable: &str, form: IterationForm) -> String {
    match form {
        IterationForm::Dotimes => format!(
            "assigning the dotimes iteration variable `{variable}`: whether that changes the \
             iteration is implementation-dependent, since dotimes may bind it afresh each time"
        ),
        IterationForm::Dolist => format!(
            "assigning the dolist iteration variable `{variable}`, which nothing reads \
             afterwards: the next iteration rebinds it from the list, so the assignment is lost"
        ),
    }
}

/// `setq`-family forms whose arguments alternate place and value.
const PAIRWISE_ASSIGNMENTS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];
/// Forms whose *first* argument is the place.
const FIRST_PLACE_ASSIGNMENTS: [&str; 3] = ["incf", "decf", "pop"];
/// Forms whose *second* argument is the place.
const SECOND_PLACE_ASSIGNMENTS: [&str; 2] = ["push", "pushnew"];
/// Forms every one of whose arguments is a place.
const ALL_PLACE_ASSIGNMENTS: [&str; 2] = ["rotatef", "shiftf"];

/// Forms that may rebind a name, and so end this walk's interest in the
/// subtree under them.
///
/// Over-broad on purpose: a `let` that binds something *else* also stops the
/// walk if the name appears anywhere in its binding list. Missing a finding is
/// cheaper than reporting an assignment to a different variable that happens
/// to share a name.
const REBINDING_FORMS: [&str; 13] = [
    "destructuring-bind",
    "do",
    "do*",
    "dolist",
    "dotimes",
    "flet",
    "labels",
    "lambda",
    "let",
    "let*",
    "multiple-value-bind",
    "symbol-macrolet",
    "with-slots",
];

/// Whether `view` is a form that may rebind `variable`.
///
/// Answered by "does the name appear anywhere in the form's binding part",
/// which for `lambda` and the `do` family is child 1, and for
/// `multiple-value-bind`/`destructuring-bind` likewise. Reading each shape's
/// lambda list exactly would be more precise and would buy nothing: the
/// conservative answer only ever suppresses.
fn rebinds(view: &ExpressionView, variable: &str) -> bool {
    if !list_head(view).is_some_and(|head| symbol_in(head, &REBINDING_FORMS)) {
        return false;
    }
    let Some(bindings) = view.children.get(1) else {
        return false;
    };
    let mut found = false;
    for_each_subview(bindings, |subview| {
        if !found && normalized_symbol(subview).as_deref() == Some(variable) {
            found = true;
        }
    });
    found
}

/// Whether `place` is the bare symbol `variable` — not `(car variable)`, not
/// `'variable`.
fn is_place(place: &ExpressionView, variable: &str) -> bool {
    plain_name(place).as_deref() == Some(variable)
}

/// Whether `view` assigns `variable` as a whole.
fn assigns(view: &ExpressionView, variable: &str) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    let arguments = &view.children[1.min(view.children.len())..];

    if symbol_in(head, &PAIRWISE_ASSIGNMENTS) {
        return arguments
            .iter()
            .step_by(2)
            .any(|place| is_place(place, variable));
    }
    if symbol_in(head, &FIRST_PLACE_ASSIGNMENTS) {
        return arguments
            .first()
            .is_some_and(|place| is_place(place, variable));
    }
    if symbol_in(head, &SECOND_PLACE_ASSIGNMENTS) {
        return arguments
            .get(1)
            .is_some_and(|place| is_place(place, variable));
    }
    if symbol_in(head, &ALL_PLACE_ASSIGNMENTS) {
        return arguments.iter().any(|place| is_place(place, variable));
    }
    if symbol_is(head, "multiple-value-setq") {
        return arguments.first().is_some_and(|places| {
            places
                .children
                .iter()
                .any(|place| is_place(place, variable))
        });
    }
    false
}

/// Whether `variable` is read anywhere in `body` after byte `offset`.
///
/// The `dolist` half of the rule turns on this: an assignment whose value the
/// rest of the iteration reads is the scratch-variable idiom, which is legal
/// and portable. An occurrence inside the assignment form itself is excluded
/// by taking the assignment's *end* as the offset.
fn read_after(body: &[ExpressionView], variable: &str, offset: usize) -> bool {
    let mut found = false;
    for form in body {
        for_each_subview(form, |subview| {
            if found || subview.span.start().get() < offset {
                return;
            }
            if normalized_symbol(subview).as_deref() == Some(variable) {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

///
/// Reads only the matched form's own subtree.
pub fn examine_iteration(
    tree: &SyntaxTree,
    view: &ExpressionView,
    iteration_form_count: &mut usize,
    violations: &mut Vec<DotimesDolistIndexVarMutatedItem>,
) {
    if !is_paren_list(view) {
        return;
    }
    let Some(form) = list_head(view).and_then(IterationForm::read) else {
        return;
    };
    *iteration_form_count += 1;

    // `(dotimes (var count [result]) body*)`. A spec that is not a list of at
    // least one plain symbol is `malformed-iteration-spec`'s subject.
    let Some(spec) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(spec) {
        return;
    }
    let Some(variable) = spec.children.first().and_then(plain_name) else {
        return;
    };
    let body = &view.children[2.min(view.children.len())..];
    if body.is_empty() {
        return;
    }

    let mut assignments = Vec::new();
    for form in body {
        for_each_evaluated_subview_where(
            form,
            |subview| !rebinds(subview, &variable),
            |subview| {
                if assigns(subview, &variable) {
                    assignments.push(subview.span);
                }
            },
        );
    }
    if assignments.is_empty() {
        return;
    }
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    for span in assignments {
        // A `dolist` assignment whose value the rest of the body reads is the
        // scratch-variable idiom, which is well-defined in every conforming
        // implementation.
        if form == IterationForm::Dolist && read_after(body, &variable, span.end().get()) {
            continue;
        }
        violations.push(DotimesDolistIndexVarMutatedItem {
            span,
            variable: variable.clone(),
            form,
        });
    }
}

/// Collects every mutated iteration variable in one file, with the number of
/// `dotimes`/`dolist` forms scanned as the denominator beside them.
pub fn build_dotimes_dolist_index_var_mutated_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DotimesDolistIndexVarMutatedItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("iteration_form_count", json!(0))],
        ));
    }

    let mut iteration_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_iteration(tree, subview, &mut iteration_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("iteration_form_count", json!(iteration_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DotimesDolistIndexVarMutatedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_dotimes_dolist_index_var_mutated_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn found(input: &str) -> Vec<(String, IterationForm)> {
        report(input)
            .findings
            .into_iter()
            .map(|item| (item.variable, item.form))
            .collect()
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_setq_of_the_dotimes_index() {
        assert_eq!(
            found("(dotimes (i 10) (setq i 20))"),
            vec![("i".to_owned(), IterationForm::Dotimes)]
        );
    }

    #[test]
    fn flags_every_assignment_spelling() {
        for source in [
            "(dotimes (i 10) (setq i 1))",
            "(dotimes (i 10) (psetq i 1))",
            "(dotimes (i 10) (setf i 1))",
            "(dotimes (i 10) (psetf i 1))",
            "(dotimes (i 10) (incf i))",
            "(dotimes (i 10) (decf i))",
            "(dotimes (i 10) (pop i))",
            "(dotimes (i 10) (push 1 i))",
            "(dotimes (i 10) (pushnew 1 i))",
            "(dotimes (i 10) (rotatef i j))",
            "(dotimes (i 10) (shiftf i j 1))",
            "(dotimes (i 10) (multiple-value-setq (i j) (foo)))",
        ] {
            assert_eq!(found(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn flags_the_second_place_of_a_pairwise_setq() {
        assert_eq!(found("(dotimes (i 10) (setq j 1 i 2))").len(), 1);
    }

    #[test]
    fn flags_an_assignment_nested_deep_in_the_body() {
        assert_eq!(
            found("(dotimes (i 10) (when (foo) (progn (setq i 20))))").len(),
            1
        );
    }

    /// The `dolist` half fires only when the assignment is discarded.
    #[test]
    fn flags_a_dolist_assignment_nothing_reads_afterwards() {
        assert_eq!(
            found("(dolist (x l) (setq x (normalize x)))"),
            vec![("x".to_owned(), IterationForm::Dolist)]
        );
    }

    #[test]
    fn the_span_covers_the_assignment_and_not_the_loop() {
        let report = report("(dotimes (i 10) (setq i 20))");
        let finding = &report.findings[0];
        assert_eq!(finding.span().start().get(), 16);
        assert_eq!(finding.span().end().get(), 27);
    }

    // -- near-miss negatives ------------------------------------------------

    /// The scratch-variable idiom: legal, portable, and read afterwards.
    #[test]
    fn does_not_flag_a_dolist_assignment_the_body_reads_afterwards() {
        assert!(found("(dolist (x l) (setq x (normalize x)) (use x))").is_empty());
    }

    /// The same shape under `dotimes` is still implementation-dependent.
    #[test]
    fn flags_a_dotimes_assignment_even_when_the_body_reads_it_afterwards() {
        assert_eq!(found("(dotimes (i 10) (setq i (1+ i)) (use i))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_body_that_only_reads_the_variable() {
        assert!(found("(dotimes (i 10) (print i))").is_empty());
        assert!(found("(dolist (x l) (print x))").is_empty());
    }

    /// The *count* variable is `dotimes-bound-mutation-has-no-effect`'s
    /// subject, not this rule's.
    #[test]
    fn does_not_flag_an_assignment_to_the_count_variable() {
        assert!(found("(dotimes (i n) (setq n 0))").is_empty());
        assert!(found("(dolist (x l) (setq l nil))").is_empty());
    }

    #[test]
    fn does_not_flag_a_mutation_of_the_object_the_variable_points_at() {
        assert!(found("(dolist (x l) (setf (car x) 1))").is_empty());
        assert!(found("(dolist (x l) (incf (gethash x h)))").is_empty());
    }

    #[test]
    fn does_not_flag_an_assignment_to_a_rebound_name() {
        for source in [
            "(dotimes (i 10) (let ((i 0)) (setq i 5)))",
            "(dotimes (i 10) (let* ((i 0)) (setq i 5)))",
            // The inner `dolist` reads `i` after assigning it, so neither the
            // outer `dotimes` (which stops at the rebinding) nor the inner one
            // (the scratch-variable idiom) has anything to say.
            "(dotimes (i 10) (dolist (i l) (setq i 5) (use i)))",
            "(dotimes (i 10) (multiple-value-bind (i j) (foo) (setq i 5)))",
            "(dotimes (i 10) (destructuring-bind (i) l (setq i 5)))",
            "(dotimes (i 10) (mapcar (lambda (i) (setq i 5)) l))",
            "(dotimes (i 10) (flet ((f (i) (setq i 5))) (f 1)))",
            "(dotimes (i 10) (symbol-macrolet ((i (aref a 0))) (setq i 5)))",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_form_with_no_body() {
        assert!(found("(dotimes (i 10))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_spec() {
        assert!(found("(dotimes i (setq i 1))").is_empty());
        assert!(found("(dotimes () (setq i 1))").is_empty());
        assert!(found("(dotimes ((i) 10) (setq i 1))").is_empty());
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert_eq!(found("(CL:DOTIMES (I 10) (CL:SETQ I 20))").len(), 1);
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert!(found("'(dotimes (i 10) (setq i 20))").is_empty());
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert!(found("(quote (dotimes (i 10) (setq i 20)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert!(found("'(a ,(dotimes (i 10) (setq i 20)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert!(found("(defmacro m () `(dotimes (i 10) (setq i 20)))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            found("(defmacro m () `(a ,(dotimes (i 10) (setq i 20))))").len(),
            1
        );
    }

    /// A quoted assignment inside an evaluated loop is data.
    #[test]
    fn does_not_flag_a_quoted_assignment_in_the_body() {
        assert!(found("(dotimes (i 10) '(setq i 20))").is_empty());
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_flag_an_assignment_inside_a_string_literal() {
        assert!(found("(dotimes (i 10) \"(setq i 20)\")").is_empty());
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(dotimes (i 10) (setq i 20))", Dialect::Clojure)
            .expect("parse");
        let report = build_dotimes_dolist_index_var_mutated_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("iteration_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_loop_scanned_not_only_the_flagged_ones() {
        let report = report("(dotimes (i 10) (setq i 1))\n(dolist (x l) (print x))\n");
        assert_eq!(report.summary, vec![("iteration_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_variable_and_its_form() {
        let report = report("(defun f ()\n  (dotimes (i 10) (setq i 20)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "dotimes-dolist-index-var-mutated");
        assert_eq!(
            finding.json_fields(),
            vec![("variable", json!("i")), ("form", json!("dotimes"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["variable=i".to_owned(), "form=dotimes".to_owned()]
        );
    }
}
