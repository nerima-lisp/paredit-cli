//! Detection for `unused-local-binding`.
//!
//! A `let`/`let*`/`flet`/`labels` binding that nothing in its body reads.
//!
//! This is the question the syntactic rules cannot ask. "Is this name read"
//! is not a property of the binder's text: it depends on which of several
//! same-named bindings each occurrence below resolves to, on whether the
//! occurrence is in operator or argument position (Common Lisp is a Lisp-2),
//! and on whether the enclosing text is quoted. `RuleContext::binding_table()`
//! answers all three, and this module is a set of guards on top of that answer.
//!
//! Every guard exists because the table's answer, taken literally, is wrong for
//! a specific and real shape of correct code. They are listed on
//! [`Suppression`].

use paredit_core_lint_engine::engine::RuleContext;
use paredit_core_semantics::semantics::binding::{Binding, BindingKind};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::support::{
    bindings_opened_by, declared_ignorable, every_occurrence_is_explained,
    is_conventionally_unused, looks_dynamically_bound, scan_name_uses,
};

/// Why a binding that has no references is nevertheless not reported.
///
/// Kept as data rather than folded into an early `continue` so the corpus
/// audit can count each one: a guard that suppresses nothing on real code is
/// either dead or missing a test, and the only way to tell is to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suppression {
    /// `(declare (ignore x))` or `(declare (ignorable x))` — the author said so.
    DeclaredIgnorable,
    /// `_` or `_x`: the conventional spelling of a deliberately unused name.
    ConventionallyUnused,
    /// `*x*`: the binding may be a rebinding of a global declared in another
    /// file, which callees read with no textual reference. See
    /// [`crate::support::looks_dynamically_bound`].
    LooksDynamic,
    /// This file declares the name special, so the same argument applies with
    /// proof rather than convention.
    DeclaredSpecial,
    /// The scope encloses a macro call, quoted region or reader conditional
    /// this layer cannot see through, so a reference may exist that no
    /// traversal can find.
    OpaqueScope,
    /// An occurrence of the name under this form is one the table neither
    /// resolved nor recognised as a definition, so its account of the name is
    /// incomplete and "no references" proves nothing. See
    /// [`crate::support::every_occurrence_is_explained`].
    UnexplainedOccurrence,
}

/// One binding that nothing reads.
#[derive(Debug, Clone)]
pub struct UnusedBinding {
    /// The name atom in the binding list.
    pub span: ByteSpan,
    pub name: String,
    /// `let`, `let*`, `flet` or `labels`.
    pub binder: String,
    pub kind: BindingKind,
}

impl UnusedBinding {
    #[must_use]
    pub fn message(&self) -> String {
        let what = match self.kind {
            BindingKind::Function => "local function",
            _ => "local binding",
        };
        format!(
            "{} `{}` bound by `{}` is never read in its body; remove it or \
             declare it ignored",
            what, self.name, self.binder
        )
    }
}

/// What one binding form yielded: the findings, the candidates considered, and
/// why each suppressed candidate was dropped.
#[derive(Debug, Default)]
pub struct Outcome {
    pub findings: Vec<UnusedBinding>,
    /// Every binding this form introduced. The denominator: a zero-finding
    /// sweep over zero candidates proves nothing.
    pub candidates: usize,
    /// Unreferenced bindings dropped by a guard, and which guard.
    pub suppressed: Vec<(String, Suppression)>,
}

/// The heads this rule matches, and whether each binds functions or variables.
#[must_use]
pub fn binder_kind(head: &str) -> Option<BindingKind> {
    if head.eq_ignore_ascii_case("let") || head.eq_ignore_ascii_case("let*") {
        Some(BindingKind::Variable)
    } else if head.eq_ignore_ascii_case("flet") || head.eq_ignore_ascii_case("labels") {
        Some(BindingKind::Function)
    } else {
        None
    }
}

/// Whether the form is worth building the binding table for.
///
/// Deliberately the *first* thing every caller does, and deliberately cheap:
/// it reads `view.children` and nothing else. Building the binding table is
/// real work, and reaching for it before this check is the ordering that cost
/// a sibling rule four orders of magnitude per call.
#[must_use]
pub fn is_candidate_form(view: &ExpressionView) -> Option<(&str, BindingKind)> {
    if view.kind != ExpressionKind::List || view.children.len() < 3 {
        // Fewer than three children means no body: `(let ((x 1)))` is
        // `empty-let`'s finding, not this rule's.
        return None;
    }
    let head = list_head(view)?;
    let kind = binder_kind(head)?;
    let binding_list = &view.children[1];
    // An empty or non-list binding list introduces nothing.
    if binding_list.kind != ExpressionKind::List || binding_list.children.is_empty() {
        return None;
    }
    Some((head, kind))
}

/// The names a binding list introduces, read syntactically.
///
/// Only used to drive the cheap pre-filter, so an approximation is fine: a
/// name this misses simply means the table is consulted, and a name it invents
/// matches nothing.
fn binding_list_names(binding_list: &ExpressionView) -> Vec<&str> {
    binding_list
        .children
        .iter()
        .filter_map(|clause| {
            if clause.kind == ExpressionKind::Atom {
                paredit_core_syntax::sexpr::reader::atom_symbol_text(clause)
            } else {
                clause
                    .children
                    .first()
                    .and_then(paredit_core_syntax::sexpr::reader::atom_symbol_text)
            }
        })
        .collect()
}

/// Whether this binding, which has no references, should nevertheless be left
/// alone — and why.
///
/// There is deliberately no `assignments()` test here. It looks like an
/// obvious guard — "reassigned but never read is a different claim" — and it
/// is unreachable: `probe_an_assignment_also_records_a_reference` shows the
/// binding table pushes a *reference* at every `setq`/`setf`/`incf`/`push`
/// site as well as an assignment, so a binding with an assignment always has a
/// reference and never reaches this function. Mutation testing found it
/// killing nothing, and the probe explained why.
fn suppression(
    context: &RuleContext<'_>,
    binding: &Binding,
    name: &str,
    ignorable: &[&str],
    view: &ExpressionView,
) -> Option<Suppression> {
    if !binding.special().is_lexical() {
        return Some(Suppression::DeclaredSpecial);
    }
    if looks_dynamically_bound(name) {
        return Some(Suppression::LooksDynamic);
    }
    if is_conventionally_unused(name) {
        return Some(Suppression::ConventionallyUnused);
    }
    if ignorable
        .iter()
        .any(|other| other.eq_ignore_ascii_case(name))
    {
        return Some(Suppression::DeclaredIgnorable);
    }
    if !binding.opacity().is_transparent() {
        return Some(Suppression::OpaqueScope);
    }
    // Last, and the only guard that reaches past the form itself, because it
    // is the only one that has to. On a real corpus it runs a few dozen times
    // per thousand files — every cheap test above has already run — so the
    // cost of the wider walk is bounded by how selective the rest are.
    //
    // The *enclosing top-level form*, not the binder: ASDF splices sibling
    // `let*` forms into one another with its `nest` macro, so
    // `(nest (let* ((latest-in …)) …) (let ((up-to-date-p (… latest-in …))) …))`
    // reads `latest-in` from a form that is the binder's *sibling* in the
    // text and its *body* after expansion. Scanning only the binder called
    // both of ASDF's bindings unused.
    let enclosing = enclosing_top_level(context, view);
    let region = enclosing.as_ref().unwrap_or(view);
    if !every_occurrence_is_explained(context.binding_table(), region, name) {
        return Some(Suppression::UnexplainedOccurrence);
    }
    None
}

/// The top-level form containing `view`.
///
/// Reaches `root_view()`, which is the expensive call this module otherwise
/// avoids — hence its position as the last test in [`suppression`], after
/// every cheap one has had its chance to reject the candidate.
fn enclosing_top_level(context: &RuleContext<'_>, view: &ExpressionView) -> Option<ExpressionView> {
    let mut root = context.tree().root_view();
    let index = root.children.iter().position(|child| {
        child.span.start().get() <= view.span.start().get()
            && view.span.end().get() <= child.span.end().get()
    })?;
    Some(root.children.swap_remove(index))
}

/// Examines one `let`/`let*`/`flet`/`labels` form.
///
/// `gate_on_opacity` is false only for the corpus audit, which measures how
/// much the transparency requirement costs and how much it saves. The rule
/// always passes true.
#[must_use]
pub fn examine(context: &RuleContext<'_>, view: &ExpressionView, gate_on_opacity: bool) -> Outcome {
    let mut outcome = Outcome::default();
    let Some((head, kind)) = is_candidate_form(view) else {
        return outcome;
    };

    // The cheap pass first, and it decides whether there is a second one.
    //
    // `binding_table()` is memoized per file, but reading it is not free: a
    // `ScopeId` cannot be constructed from outside `core/semantics`, so the
    // only way to ask "which bindings did *this* form open" is to scan every
    // binding in the file. Doing that per `let` is quadratic — measured at
    // 125 us per call on a 400 KB file, against 0.6 us for a no-op rule with
    // the same `HeadFilter`.
    //
    // So the syntactic scan below answers first. A name that occurs more than
    // once under the form is read by something, and a name that occurs once is
    // read by nothing; either way the table cannot change the verdict. It can
    // only matter when a nested binder rebinds the name, which is exactly the
    // shadowing case this rule exists for. That leaves about one candidate
    // form in twenty reaching the table.
    let names: Vec<&str> = binding_list_names(&view.children[1]);
    if names.is_empty() {
        return outcome;
    }
    let uses = scan_name_uses(view, &names, kind == BindingKind::Function);
    let interesting = uses.iter().any(|use_of| use_of.needs_the_table());

    if !interesting {
        // Still count the denominator: a candidate rejected cheaply is a
        // candidate examined, and a rate reported against the wrong
        // denominator is worse than none.
        outcome.candidates += names.len();
        return outcome;
    }

    let ignorable = declared_ignorable(&view.children[2..]);

    for binding in bindings_opened_by(context.binding_table(), view.span) {
        if binding.kind() != kind {
            continue;
        }
        outcome.candidates += 1;
        if !binding.references().is_empty() {
            continue;
        }
        let name = binding.name().as_str();
        match suppression(context, binding, name, &ignorable, view) {
            Some(Suppression::OpaqueScope) if !gate_on_opacity => {}
            Some(reason) => {
                outcome.suppressed.push((name.to_owned(), reason));
                continue;
            }
            None => {}
        }
        outcome.findings.push(UnusedBinding {
            span: binding.definition(),
            name: name.to_owned(),
            binder: head.to_owned(),
            kind,
        });
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_core_syntax::view_query::for_each_subview;
    use std::path::Path;

    fn run(source: &str) -> Outcome {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(Path::new("t.lisp"), Dialect::CommonLisp, &tree, source);
        let mut total = Outcome::default();
        let root = tree.root_view();
        for child in &root.children {
            for_each_subview(child, |view| {
                let outcome = examine(&context, view, true);
                total.findings.extend(outcome.findings);
                total.candidates += outcome.candidates;
                total.suppressed.extend(outcome.suppressed);
            });
        }
        total
    }

    fn names(source: &str) -> Vec<String> {
        run(source).findings.into_iter().map(|f| f.name).collect()
    }

    #[test]
    fn reports_a_let_binding_nothing_reads() {
        assert_eq!(names("(defun f () (let ((x 1)) (list 2)))"), vec!["x"]);
    }

    #[test]
    fn does_not_report_a_binding_the_body_reads() {
        assert!(names("(defun f () (let ((x 1)) (list x)))").is_empty());
    }

    #[test]
    fn reports_only_the_unread_one_of_several() {
        assert_eq!(
            names("(defun f () (let ((x 1) (y 2) (z 3)) (list y z)))"),
            vec!["x"]
        );
    }

    #[test]
    fn reports_an_unread_flet_function() {
        assert_eq!(names("(defun f () (flet ((g () 1)) (list 2)))"), vec!["g"]);
    }

    #[test]
    fn does_not_report_an_flet_used_in_operator_position() {
        assert!(names("(defun f () (flet ((g () 1)) (g)))").is_empty());
    }

    /// The binding table drops `#'g` outright, so `references()` is empty and
    /// only the completeness guard stops this. Deleting `g` on that advice
    /// would break the program.
    #[test]
    fn does_not_report_an_flet_referenced_by_function_quote() {
        assert!(names("(defun f () (flet ((g () 1)) (mapcar #'g nil)))").is_empty());
    }

    #[test]
    fn does_not_report_an_flet_referenced_by_the_long_hand_function_form() {
        assert!(names("(defun f () (flet ((g () 1)) (mapcar (function g) nil)))").is_empty());
    }

    #[test]
    fn does_not_report_a_labels_sibling_referenced_by_function_quote() {
        assert!(names("(defun f () (labels ((g () 1) (h () (mapcar #'g nil))) (h)))").is_empty());
    }

    /// A variable spliced into operator position of a macro template is looked
    /// up in the function namespace and missed. SBCL's own
    /// `target-sxhash.lisp` does exactly this, and the rule reported it until
    /// the completeness guard existed.
    #[test]
    fn does_not_report_a_variable_spliced_into_operator_position_of_a_template() {
        assert!(
            names("(defmacro m (type) (let ((hasher (mangle type))) `(,hasher key)))").is_empty()
        );
    }

    #[test]
    fn does_not_report_a_variable_spliced_into_a_template_argument() {
        assert!(names("(defmacro m () (let ((idx '#:i)) `(loop for ,idx from 1)))").is_empty());
    }

    /// The completeness guard must not swallow the plain case: with no
    /// unexplained occurrence anywhere, the finding still stands.
    #[test]
    fn the_completeness_guard_leaves_a_plainly_unused_binding_reported() {
        assert_eq!(names("(defun f () (let ((x 1)) (list 2)))"), vec!["x"]);
    }

    /// A package-qualified dynamic variable must be recognised as one. Eight of
    /// the first corpus run's twenty findings were this shape.
    #[test]
    fn does_not_report_a_package_qualified_earmuffed_rebinding() {
        assert!(names("(defun f (v) (let ((sb-debug:*stack-top-hint* v)) (g)))").is_empty());
    }

    /// A Lisp-2 fact a syntactic rule gets wrong: the `x` in argument position
    /// is a *variable* reference and does not read the `flet`.
    #[test]
    fn a_value_reference_does_not_count_as_a_use_of_a_local_function() {
        assert_eq!(names("(defun f (x) (flet ((x () 1)) (list x)))"), vec!["x"]);
    }

    /// The shadowing fact a text search gets wrong: the inner `x` takes the
    /// reference, so the *outer* binding is the unused one.
    #[test]
    fn reports_the_shadowed_outer_binding_not_the_inner_one() {
        let findings = run("(defun f () (let ((x 1)) (let ((x 2)) (list x))))").findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].name, "x");
        // The outer binder's name atom, at offset 19, not the inner at 31.
        assert_eq!(findings[0].span.start().get(), 19);
    }

    #[test]
    fn does_not_report_a_binding_read_only_from_a_nested_scope() {
        assert!(names("(defun f () (let ((x 1)) (let ((y 2)) (list x y))))").is_empty());
    }

    // -- guards --------------------------------------------------------------

    #[test]
    fn does_not_report_a_declared_ignore() {
        assert!(names("(defun f () (let ((x 1)) (declare (ignore x)) 2))").is_empty());
    }

    #[test]
    fn does_not_report_a_declared_ignorable() {
        assert!(names("(defun f () (let ((x 1)) (declare (ignorable x)) 2))").is_empty());
    }

    #[test]
    fn does_not_report_an_underscore_name() {
        assert!(names("(defun f () (let ((_x 1)) 2))").is_empty());
    }

    #[test]
    fn does_not_report_an_earmuffed_rebinding() {
        assert!(names("(defun f (s) (let ((*standard-output* s)) (print 1)))").is_empty());
    }

    #[test]
    fn does_not_report_a_binding_this_file_declares_special() {
        assert!(names("(defvar foo)\n(defun f () (let ((foo 1)) (bar)))").is_empty());
    }

    #[test]
    fn does_not_report_a_binding_that_is_assigned() {
        assert!(names("(defun f () (let ((acc nil)) (setf acc 1)))").is_empty());
    }

    #[test]
    fn does_not_report_when_an_unknown_macro_could_capture_it() {
        assert!(names("(defun f () (let ((x 1)) (my-anaphoric-macro)))").is_empty());
    }

    #[test]
    fn does_not_report_a_binder_with_no_body() {
        // `empty-let`'s finding, not this rule's.
        assert!(names("(defun f () (let ((x 1))))").is_empty());
    }

    #[test]
    fn does_not_report_inside_a_macro_template() {
        assert!(names("(defmacro m () `(let ((x 1)) (list 2)))").is_empty());
    }

    /// An assigned binding is never reported, because the table records a
    /// reference at the assignment site — not because of any guard here. See
    /// the note on [`suppression`].
    #[test]
    fn does_not_report_an_assigned_binding_reached_through_the_shadowing_path() {
        assert!(
            names("(defun f () (let ((acc nil)) (setf acc 1) (let ((acc 2)) (list acc acc))))")
                .is_empty()
        );
    }

    /// `rebound_below` earns its place only when the *inner* binding is read
    /// more than once: one read leaves the outer name at a single occurrence,
    /// which reaches the table anyway. Two reads do not, so without the
    /// rebinding signal the outer unused binding is missed entirely.
    #[test]
    fn reports_a_shadowed_outer_binding_whose_inner_twin_is_read_twice() {
        assert_eq!(
            names("(defun f () (let ((x 1)) (let ((x 2)) (list x x))))"),
            vec!["x"]
        );
    }

    /// A reader conditional inside a macro template folds into one opaque atom
    /// *and* is never reached by the opacity marking, because references are
    /// not recorded at quasiquote depth. Only the buried-symbol scan stops
    /// this. SBCL's `target-sxhash.lisp` is this shape.
    #[test]
    fn does_not_report_a_name_read_only_from_a_blob_inside_a_template() {
        assert!(
            names(
                "(defmacro m (type) (let ((hasher (mangle type))) \
                 `(cond (t 1) #+64-bit (t (funcall hasher 2)))))"
            )
            .is_empty()
        );
    }

    /// A binder whose body is extended by an enclosing unknown macro reads its
    /// names from a form that is its *sibling* in the text. ASDF's `nest` does
    /// exactly this, and scanning only the binder called both of its bindings
    /// unused.
    #[test]
    fn does_not_report_a_binding_read_from_a_sibling_form_spliced_by_a_macro() {
        // Every head inside the `let*` is a known one, so the `OpaqueScope`
        // guard does not fire and the widening is what has to catch this.
        assert!(
            names(
                "(defun f (o c) (nest (let* ((latest-in (car o)) (other (car c))) \
                 (list other)) (let ((up (list latest-in))) (list up))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn counts_every_binding_as_a_candidate_not_only_the_reported_ones() {
        let outcome = run("(defun f () (let ((x 1) (y 2)) (list y)))");
        assert_eq!(outcome.candidates, 2);
        assert_eq!(outcome.findings.len(), 1);
    }

    /// A name mentioned by *two* declarations is still a name nothing reads.
    /// Without the pre-filter's declaration skip the two mentions read as two
    /// uses, the candidate never reaches the table, and the `DeclaredIgnorable`
    /// guard never gets to explain itself.
    #[test]
    fn declarations_are_not_reads_however_many_mention_the_name() {
        let outcome =
            run("(defun f () (let ((x 1)) (declare (type fixnum x)) (declare (ignore x)) 2))");
        assert!(outcome.findings.is_empty());
        assert_eq!(
            outcome.suppressed,
            vec![("x".to_owned(), Suppression::DeclaredIgnorable)]
        );
    }

    #[test]
    fn records_which_guard_suppressed_a_candidate() {
        let outcome = run("(defun f () (let ((x 1)) (declare (ignore x)) 2))");
        assert_eq!(
            outcome.suppressed,
            vec![("x".to_owned(), Suppression::DeclaredIgnorable)]
        );
    }
}
