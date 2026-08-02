//! A special rebound by `let` and then read inside a thread started in that
//! `let`'s body.
//!
//! A dynamic binding belongs to the thread that established it. A new thread
//! starts with the *global* value of every special, so
//!
//! ```lisp
//! (let ((*database* (connect-to-replica)))
//!   (bt:make-thread (lambda () (query *database*))))
//! ```
//!
//! does not query the replica: the worker sees whatever `*database*` was bound
//! to at top level, which is usually the wrong connection and sometimes
//! unbound. This is the single most common threading bug in Common Lisp, it
//! produces no error, and reading the code does not reveal it — the binding and
//! the use look adjacent.
//!
//! The repair is to capture the value lexically before spawning and rebind
//! inside the thunk. Which of those two the author wants, and under what name,
//! is a design decision, so this rule reports and does not rewrite.
//!
//! # The shape it requires, all of it
//!
//! 1. A `make-thread` whose function argument is a literal `lambda` — the
//!    closure has to be written here for its captures to be visible.
//! 2. The spawn carries **no** `:initial-bindings` and no `:arguments`. Both
//!    are evaluated in the *spawning* thread and are the documented ways to
//!    carry a value across the boundary, so a spawn using either has already
//!    answered the question this rule asks.
//! 3. An enclosing `let`/`let*` that binds an `*earmuffed*` name.
//! 4. That exact name read somewhere inside the thunk.
//! 5. The thunk does **not** rebind it itself — by `let`/`let*`, by `progv`, or
//!    by naming it in its own lambda list.
//!
//! Point 5 is the correctness guard that makes this rule usable. The idiomatic
//! repair —
//!
//! ```lisp
//! (let ((*database* (connect-to-replica)))
//!   (let ((db *database*))
//!     (bt:make-thread (lambda () (let ((*database* db)) (query *database*))))))
//! ```
//!
//! — still mentions `*database*` inside the thunk, and a rule without point 4
//! would report the fix as the bug.
//!
//! # What it does not attempt
//!
//! - **Proving the name is special.** Earmuffs are a convention. A `defvar` in
//!   another file cannot be seen, so the convention is the only signal, and a
//!   lexical variable spelled with earmuffs would be a naming violation of its
//!   own.
//! - **Distinguishing a read from a write.** Any mention of the name inside the
//!   thunk that is not a rebinding is treated as a use. A write to it is the
//!   neighbouring `unsynchronized-shared-mutation`'s subject, and both being
//!   wrong about the same form is the correct outcome.
//! - **Values passed in another way.** `(let ((db *database*)) (make-thread
//!   (lambda () (query db))))` mentions `db`, not `*database*`, and is silent —
//!   which is right, because that code is correct.
//! - **Threads started outside the `let`.** The ancestor walk only sees what
//!   lexically encloses the spawn.
//! - **A thunk built by a helper.** If the function is produced by some other
//!   form that re-establishes bindings, requirement 1 keeps the rule off it.
//!
//! Scope: Common Lisp only. Clojure's `binding` conveys to `future` bodies by
//! design, which is the opposite property, so applying this there would be
//! wrong.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, head_is, looks_special, symbol_name, with_ancestor_chain,
};

/// The spawn this rule reads. Spelled the same by `bordeaux-threads` and
/// `sb-thread`.
pub const SPAWN_HEADS: &[&str] = &["make-thread"];

#[derive(Debug, Clone)]
pub struct DynamicVarBoundAcrossThreadBoundaryItem {
    /// The span of the whole spawn form.
    pub span: ByteSpan,
    /// The special that the enclosing `let` rebinds and the thunk reads.
    pub variable: String,
}

impl Finding for DynamicVarBoundAcrossThreadBoundaryItem {
    fn kind(&self) -> &'static str {
        "dynamic-var-bound-across-thread-boundary"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("variable={}", self.variable)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("variable", json!(self.variable))]
    }

    fn message(&self) -> String {
        format!(
            "the new thread reads {}, but a let binding does not cross a thread boundary, so it \
             sees the global value",
            self.variable
        )
    }
}

/// The earmuffed names a `let`/`let*` binds.
fn bound_specials(view: &ExpressionView, out: &mut Vec<String>) {
    if !head_is(view, &["let", "let*"]) {
        return;
    }
    let Some(bindings) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(bindings) {
        return;
    }
    for binding in &bindings.children {
        let name = if is_paren_list(binding) {
            binding.children.first().and_then(symbol_name)
        } else {
            symbol_name(binding)
        };
        if let Some(name) = name.filter(|name| looks_special(name)) {
            out.push(name);
        }
    }
}

/// Every earmuffed name in a subtree, quoted data included — for `progv`, whose
/// variable list is normally quoted.
fn earmuffed_symbols_in(view: &ExpressionView, out: &mut Vec<String>) {
    paredit_core_syntax::view_query::for_each_subview(view, |node| {
        if let Some(name) = symbol_name(node).filter(|name| looks_special(name)) {
            out.push(name);
        }
    });
}

/// Whether the spawn carries a keyword argument that conveys state to the new
/// thread.
///
/// `bt:make-thread`'s `:initial-bindings` is *the* documented way to carry a
/// dynamic binding across the boundary, and `:arguments` passes values that are
/// evaluated in the spawning thread. A spawn using either has already answered
/// this rule's question, and the finding's sentence — "a let binding does not
/// cross a thread boundary" — would simply be false about it.
fn conveys_state_explicitly(spawn: &ExpressionView) -> bool {
    spawn.children.iter().skip(2).any(|argument| {
        symbol_name(argument)
            .is_some_and(|name| name == ":initial-bindings" || name == ":arguments")
    })
}

/// The `lambda` form a spawn's function argument is written as, if it is.
fn literal_thunk(spawn: &ExpressionView) -> Option<&ExpressionView> {
    let argument = spawn.children.get(1)?;
    list_head(argument)
        .is_some_and(|head| symbol_is(head, "lambda"))
        .then_some(argument)
}

/// The specials `thunk` reads, and the ones it rebinds for itself.
///
/// A rebinding is the documented repair for this defect, so a name that appears
/// only as a binding target is not a use of the enclosing binding.
fn read_and_rebound(thunk: &ExpressionView) -> (Vec<String>, Vec<String>) {
    let mut rebound = Vec::new();
    let mut read = Vec::new();
    // The thunk's own lambda list binds too: `(lambda (*connection*) …)`
    // establishes the binding on the thread that runs it.
    if let Some(parameters) = thunk.children.get(1).filter(|list| is_paren_list(list)) {
        earmuffed_symbols_in(parameters, &mut rebound);
    }
    for_each_evaluated_subview(thunk, |node| {
        bound_specials(node, &mut rebound);
        if head_is(node, &["progv"]) {
            if let Some(variables) = node.children.get(1) {
                earmuffed_symbols_in(variables, &mut rebound);
            }
        }
        if head_is(node, &["lambda", "flet", "labels"]) {
            if let Some(parameters) = node.children.get(1).filter(|list| is_paren_list(list)) {
                earmuffed_symbols_in(parameters, &mut rebound);
            }
        }
        if node.kind == ExpressionKind::Atom {
            if let Some(name) = symbol_name(node).filter(|name| looks_special(name)) {
                read.push(name);
            }
        }
    });
    (read, rebound)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Needs the tree because the enclosing `let` is what makes this a defect, and
/// [`paredit_core_lint_engine::engine::RuleContext`] carries no parent pointer.
/// The ancestor walk costs the node's depth and runs only once a spawn with a
/// literal thunk has already matched.
pub fn examine_spawn(
    tree: &SyntaxTree,
    view: &ExpressionView,
    spawn_count: &mut usize,
    violations: &mut Vec<DynamicVarBoundAcrossThreadBoundaryItem>,
) {
    if !head_is(view, SPAWN_HEADS) {
        return;
    }
    *spawn_count += 1;

    if conveys_state_explicitly(view) {
        return;
    }
    let Some(thunk) = literal_thunk(view) else {
        return;
    };
    let (read, rebound) = read_and_rebound(thunk);
    if read.is_empty() {
        return;
    }

    let Some(mut captured) = with_ancestor_chain(tree, view.span, |chain| {
        let mut names = Vec::new();
        for ancestor in chain {
            bound_specials(ancestor, &mut names);
        }
        names
    }) else {
        return;
    };
    captured.retain(|name| read.contains(name) && !rebound.contains(name));
    captured.dedup();

    for variable in captured {
        violations.push(DynamicVarBoundAcrossThreadBoundaryItem {
            span: view.span,
            variable,
        });
    }
}

/// Collects every thread body reading a `let`-rebound special in one file, with
/// the number of spawn forms scanned as the denominator beside them.
pub fn build_dynamic_var_bound_across_thread_boundary_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DynamicVarBoundAcrossThreadBoundaryItem>> {
    let mut spawn_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_spawn(tree, view, &mut spawn_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("thread_spawn_count", json!(spawn_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DynamicVarBoundAcrossThreadBoundaryItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_dynamic_var_bound_across_thread_boundary_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<DynamicVarBoundAcrossThreadBoundaryItem> {
        report(input).findings
    }

    fn variables(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.variable)
            .collect()
    }

    #[test]
    fn flags_a_thunk_reading_a_special_the_enclosing_let_rebinds() {
        assert_eq!(
            variables(
                "(let ((*database* (connect-to-replica)))\n\
                 \x20 (bt:make-thread (lambda () (query *database*))))"
            ),
            vec!["*database*".to_owned()]
        );
    }

    #[test]
    fn flags_through_let_star_and_through_several_enclosing_levels() {
        assert_eq!(
            variables("(let* ((*ctx* 1)) (progn (make-thread (lambda () (use *ctx*)))))"),
            vec!["*ctx*".to_owned()]
        );
    }

    #[test]
    fn flags_each_captured_special_once() {
        let found =
            variables("(let ((*a* 1) (*b* 2))\n  (make-thread (lambda () (use *a*) (use *b*))))");
        assert_eq!(found, vec!["*a*".to_owned(), "*b*".to_owned()]);
    }

    // --- correct code, including the documented repair ---------------------

    /// The repair. It still mentions the special inside the thunk, which is
    /// exactly why the rebinding guard has to exist.
    #[test]
    fn does_not_flag_a_thunk_that_rebinds_the_special_for_itself() {
        assert!(
            violations(
                "(let ((*database* (connect-to-replica)))\n\
                 \x20 (let ((db *database*))\n\
                 \x20   (bt:make-thread (lambda () (let ((*database* db)) (query *database*))))))"
            )
            .is_empty()
        );
    }

    /// `:initial-bindings` is bordeaux-threads' documented mechanism for
    /// carrying a dynamic binding into the new thread; the value form is
    /// evaluated in the spawning thread. The finding's sentence would be false.
    #[test]
    fn does_not_flag_a_spawn_that_conveys_its_bindings_explicitly() {
        assert!(
            violations(
                "(let ((*database* (connect-to-replica)))\n\
                 \x20 (bt:make-thread (lambda () (query *database*))\n\
                 \x20                 :initial-bindings (list (cons '*database* *database*))))"
            )
            .is_empty()
        );
    }

    /// Binding a special as a lambda parameter rebinds it on the callee's
    /// thread, and `:arguments` is evaluated in the caller's.
    #[test]
    fn does_not_flag_a_special_bound_as_the_thunks_own_parameter() {
        assert!(
            violations(
                "(let ((*connection* (open-connection)))\n\
                 \x20 (sb-thread:make-thread (lambda (*connection*) (serve *connection*))\n\
                 \x20                        :arguments (list *connection*)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_special_rebound_by_progv_in_the_thunk() {
        assert!(
            violations(
                "(let ((*ctx* 1))\n\
                 \x20 (let ((v *ctx*))\n\
                 \x20   (make-thread (lambda () (progv '(*ctx*) (list v) (use *ctx*))))))"
            )
            .is_empty()
        );
    }

    /// The other repair: capture the value lexically and never name the special
    /// inside the thunk.
    #[test]
    fn does_not_flag_a_value_captured_into_a_lexical_variable() {
        assert!(
            violations(
                "(let ((*database* (connect-to-replica)))\n\
                 \x20 (let ((db *database*))\n\
                 \x20   (bt:make-thread (lambda () (query db)))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_special_that_no_let_rebinds() {
        assert!(violations("(make-thread (lambda () (query *database*)))").is_empty());
        assert!(violations("(defun f () (make-thread (lambda () (query *database*))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_let_that_binds_a_different_special() {
        assert!(
            violations("(let ((*other* 1)) (make-thread (lambda () (query *database*))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_lexical_binding() {
        assert!(violations("(let ((db 1)) (make-thread (lambda () (query db))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_thread_whose_function_is_named_rather_than_written() {
        assert!(violations("(let ((*ctx* 1)) (make-thread #'worker))").is_empty());
    }

    #[test]
    fn does_not_flag_a_let_whose_body_starts_no_thread() {
        assert!(violations("(let ((*ctx* 1)) (query *ctx*))").is_empty());
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(let ((*ctx* 1)) (make-thread (lambda () (use *ctx*))))").is_empty());
        assert!(
            violations("(quote (let ((*ctx* 1)) (make-thread (lambda () (use *ctx*)))))")
                .is_empty()
        );
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_a_literal_comma_and_stays_data() {
        assert!(
            violations("'(a ,(let ((*ctx* 1)) (make-thread (lambda () (use *ctx*)))))").is_empty()
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(let ((*ctx* 1)) (make-thread (lambda () (use *ctx*))))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            variables("`(progn ,(let ((*ctx* 1)) (make-thread (lambda () (use *ctx*)))))"),
            vec!["*ctx*".to_owned()]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(
            violations("(format t \"(let ((*ctx* 1)) (make-thread (lambda () *ctx*)))\")")
                .is_empty()
        );
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn the_summary_counts_every_spawn_scanned() {
        let report =
            report("(make-thread #'w)\n(let ((*c* 1)) (make-thread (lambda () (use *c*))))\n");
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_variable() {
        let report = report("(let ((*ctx* 1))\n  (make-thread (lambda () (use *ctx*))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "dynamic-var-bound-across-thread-boundary");
        assert_eq!(finding.json_fields(), vec![("variable", json!("*ctx*"))]);
        assert_eq!(finding.text_columns(), vec!["variable=*ctx*".to_owned()]);
        assert_eq!(
            finding.message(),
            "the new thread reads *ctx*, but a let binding does not cross a thread boundary, so \
             it sees the global value"
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(make-thread (fn [] 1))", Dialect::Clojure)
            .expect("parse");
        let report = build_dynamic_var_bound_across_thread_boundary_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(0))]);
    }
}
