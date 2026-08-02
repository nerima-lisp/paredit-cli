//! A process-global written from inside a thread body with nothing serializing
//! the write.
//!
//! `(bt:make-thread (lambda () (incf *counter*)))` is a data race: `*counter*`
//! is one cell shared by every thread, `incf` is a read-modify-write, and
//! nothing here makes that pair atomic. The repair is a lock, an atomic
//! operation, or not sharing the cell — all design decisions, which is why this
//! rule is report-only.
//!
//! # What it looks at
//!
//! Only inside a `make-thread` whose function argument is a *literal* `lambda`
//! (with or without a `#'` prefix). That is the only shape where the code that
//! will run on the new thread is visible here. `(make-thread #'worker)` names a
//! function defined elsewhere and is never flagged.
//!
//! Within that thunk, a write is flagged when its target is an `*earmuffed*`
//! name — the convention every Common Lisp style guide reserves for a special,
//! and the same test `lint-safety`'s `global-mutation-in-function` uses.
//!
//! # What it does not attempt
//!
//! - **Anything a lock already covers.** The walk stops at any `with-…` form —
//!   the library's own `with-lock-held`/`with-mutex`/`with-recursive-lock`/
//!   `with-locked-hash-table`, Clojure's `locking`, *and* a project-local
//!   wrapper such as `(with-registry-lock …)`, which is how almost every real
//!   codebase spells it. Treating every `with-…` as protective is the same
//!   convention `lock-acquired-not-released` uses, and it only ever makes this
//!   rule quieter. Note that only a lock *inside* the thunk counts, and that is
//!   correct — a lock held by the spawning thread is not held by the thread
//!   being spawned.
//! - **A thunk that locks by hand.** If `acquire-lock` or `grab-mutex` appears
//!   anywhere in the thunk, the whole thunk is skipped. A hand-held lock's
//!   extent is not readable off the tree the way a scope's is, and the
//!   canonical `(acquire-lock l)` + `unwind-protect` + `(release-lock l)` shape
//!   — which `lock-acquired-not-released` explicitly endorses — protects the
//!   write just as well as a scope does. Judging the acquisition is that rule's
//!   job, not this one's.
//! - **A special the thunk rebinds itself,** by any of the three ways Common
//!   Lisp establishes a dynamic binding: `let`/`let*`, `progv` (whose variable
//!   list is usually quoted data), and a lambda list — `(lambda (*connection*)
//!   …)` rebinds `*connection*` on the thread that runs it. Every earmuffed
//!   name bound anywhere in the thunk is exempt, over-approximating in the
//!   silent direction.
//! - **Atomics.** `sb-ext:atomic-incf` and friends are simply not mutators as
//!   far as this rule is concerned, so they never reach it.
//! - **Proving the name is special.** Earmuffs are a convention, not a
//!   declaration. A `defvar` in another file cannot be seen from here, and a
//!   local named `*x*` would be a naming-convention violation of its own.
//! - **Nested spawns.** A `make-thread` inside the thunk is its own candidate
//!   and the walk stops there, so one write is reported once.
//!
//! # Relationship to `global-mutation-in-function`
//!
//! `lint-safety`'s `global-mutation-in-function` flags the same write with no
//! thread and no lock condition at all — it fires on every earmuffed write
//! inside any `defun`/`defmethod`/`defgeneric`/`lambda`, including one under a
//! held lock. This rule makes the strictly stronger claim: the write is on a
//! *new thread* and *nothing serializes it*. The two co-fire on the plain
//! shape; only this one is silent once a lock appears.
//!
//! **Both firing at once is expected, not a duplicate.** A `make-thread` thunk
//! is a `lambda`, which is one of that rule's heads, so every finding this rule
//! makes is also one of its findings. The two sentences say different things —
//! "this name looks like a special variable" versus "this write happens on a
//! new thread with no lock held" — and the second is the one that tells the
//! reader what to do, so it is not suppressed in favour of the shorter claim.
//! What *is* avoided is the two landing on one identical range: that rule spans
//! the whole `(setf *counter* …)`, this one spans the `*counter*` reference
//! inside it. They stay two visibly separate findings about one line.
//!
//! Both rules read "special-looking" with the same earmuff heuristic, written
//! out twice; [`crate::support::looks_special`] documents why the copy exists
//! and that the two must be changed together.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    MANUAL_ACQUIRE_HEADS, for_each_evaluated_subview, for_each_evaluated_subview_where, head_is,
    is_lock_scope, looks_special, symbol_name,
};

/// The only spawn form this rule reads, because it is the only one whose thunk
/// is written where the write can be seen. Spelled the same by
/// `bordeaux-threads` and `sb-thread`.
pub const SPAWN_HEADS: &[&str] = &["make-thread"];

#[derive(Debug, Clone)]
pub struct UnsynchronizedSharedMutationItem {
    /// The span of the *written variable reference* — the `*counter*` in
    /// `(incf *counter*)` — not of the writing form and not of the thread.
    ///
    /// Narrower than the enclosing form on purpose. `global-mutation-in-function`
    /// reports the whole `(setf *counter* …)`, and on the plain shape the two
    /// rules co-fire; pointing at the variable keeps the two findings visibly
    /// distinct instead of stacking them on one identical span, and it names
    /// the shared cell the message is actually about. It also splits
    /// `(setf *a* 1 *b* 2)` into two findings at two places rather than two at
    /// one.
    pub span: ByteSpan,
    /// The earmuffed name that is written, normalized.
    pub variable: String,
}

impl Finding for UnsynchronizedSharedMutationItem {
    fn kind(&self) -> &'static str {
        "unsynchronized-shared-mutation"
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
            "{} is written on a new thread with no lock held, so concurrent writes race",
            self.variable
        )
    }
}

/// Where a mutating operator keeps the place it writes.
///
/// Deliberately exact rather than "scan every argument": `(push *item*
/// stack)` only *reads* `*item*`, and an over-approximation would report it.
#[derive(Debug, Clone, Copy)]
enum Places {
    /// `setf`/`setq`: alternating place/value pairs from index 1.
    Odd,
    /// The single place at this argument index.
    At(usize),
    /// `rotatef`/`shiftf`: every argument is a place.
    All,
}

fn places_of(head: &str) -> Option<Places> {
    if symbol_in(head, &["setf", "setq"]) {
        return Some(Places::Odd);
    }
    if symbol_in(head, &["incf", "decf", "pop", "remf"]) {
        return Some(Places::At(1));
    }
    if symbol_in(head, &["push", "pushnew"]) {
        return Some(Places::At(2));
    }
    if symbol_in(head, &["rotatef", "shiftf"]) {
        return Some(Places::All);
    }
    None
}

/// The earmuffed names a place writes, each with the span of the reference
/// itself.
///
/// A bare `*counter*` is one. A composite place is read one level deep, so
/// `(gethash key *table*)` names `*table*` — the table is the shared cell — but
/// nothing deeper is guessed at.
///
/// The span travels with the name because the finding points at the reference,
/// not at the enclosing form; see [`UnsynchronizedSharedMutationItem::span`].
fn written_specials(place: &ExpressionView, out: &mut Vec<(String, ByteSpan)>) {
    if let Some(name) = symbol_name(place) {
        if looks_special(&name) {
            out.push((name, place.span));
        }
        return;
    }
    if !is_paren_list(place) {
        return;
    }
    for child in &place.children {
        if let Some(name) = symbol_name(child).filter(|name| looks_special(name)) {
            out.push((name, child.span));
        }
    }
}

/// Every earmuffed name in a subtree, quoted data included.
///
/// Used for `progv`, whose variable list is a quoted list of symbols
/// (`(progv '(*depth*) '(0) …)`) — the names are data there, so the evaluated
/// walk would not see them.
fn earmuffed_symbols_in(view: &ExpressionView, out: &mut Vec<String>) {
    paredit_core_syntax::view_query::for_each_subview(view, |node| {
        if let Some(name) = symbol_name(node).filter(|name| looks_special(name)) {
            out.push(name);
        }
    });
}

/// The earmuffed names `thunk` rebinds for itself, by any of the three ways
/// Common Lisp establishes a dynamic binding.
///
/// A rebinding makes the name thread-local for the duration, so a write to it
/// is not a write to the shared cell. Collected over the whole thunk rather
/// than per scope: exempting slightly too much is the safe direction.
///
/// The three ways are `let`/`let*`, `progv` (whose variable list is computed,
/// and usually quoted), and a lambda list — binding a special as a parameter
/// rebinds it exactly as `let` does.
fn rebound_specials(thunk: &ExpressionView) -> Vec<String> {
    let mut names = Vec::new();
    for_each_evaluated_subview(thunk, |view| {
        if head_is(view, &["let", "let*"]) {
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
                    names.push(name);
                }
            }
            return;
        }
        if head_is(view, &["progv"]) {
            if let Some(variables) = view.children.get(1) {
                earmuffed_symbols_in(variables, &mut names);
            }
            return;
        }
        // A lambda list: `(lambda (*connection*) …)` binds the special on the
        // thread that runs the lambda.
        if head_is(view, &["lambda", "defun", "flet", "labels"]) {
            if let Some(parameters) = view.children.get(1) {
                if is_paren_list(parameters) {
                    earmuffed_symbols_in(parameters, &mut names);
                }
            }
        }
    });
    names
}

/// The `lambda` form a spawn's function argument is written as, if it is
/// written as one at all.
///
/// `#'(lambda …)` carries a `Function` reader prefix on the same list node, so
/// both spellings land here; `#'worker` is an *atom* and does not.
fn literal_thunk(spawn: &ExpressionView) -> Option<&ExpressionView> {
    let argument = spawn.children.get(1)?;
    list_head(argument)
        .is_some_and(|head| symbol_is(head, "lambda"))
        .then_some(argument)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_spawn(
    view: &ExpressionView,
    spawn_count: &mut usize,
    violations: &mut Vec<UnsynchronizedSharedMutationItem>,
) {
    if !head_is(view, SPAWN_HEADS) {
        return;
    }
    *spawn_count += 1;

    let Some(thunk) = literal_thunk(view) else {
        return;
    };
    // A thunk that takes a lock by hand is synchronizing something, and where
    // that lock is held cannot be read off the tree the way a scope's extent
    // can. Rather than guess at the extent, say nothing about the whole thunk —
    // `lock-acquired-not-released` is the rule that judges the acquisition
    // itself, and it endorses `(acquire-lock l)` followed by an
    // `unwind-protect`, so this rule must not call the write inside one a race.
    let mut takes_a_lock_by_hand = false;
    for_each_evaluated_subview(thunk, |node| {
        takes_a_lock_by_hand = takes_a_lock_by_hand || head_is(node, MANUAL_ACQUIRE_HEADS);
    });
    if takes_a_lock_by_hand {
        return;
    }
    let rebound = rebound_specials(thunk);

    for_each_evaluated_subview_where(
        thunk,
        // A lock scope serializes what is under it — including a project-local
        // `with-…` wrapper around the library's own macro, which is how almost
        // every codebase spells it. A nested spawn is its own candidate.
        // Neither subtree belongs to this thread body's verdict.
        |node| !is_lock_scope(node) && !head_is(node, SPAWN_HEADS),
        |node| {
            let Some(places) = list_head(node).and_then(places_of) else {
                return;
            };
            let mut written = Vec::new();
            match places {
                Places::Odd => {
                    for (index, child) in node.children.iter().enumerate().skip(1) {
                        if index % 2 == 1 {
                            written_specials(child, &mut written);
                        }
                    }
                }
                Places::At(index) => {
                    if let Some(place) = node.children.get(index) {
                        written_specials(place, &mut written);
                    }
                }
                Places::All => {
                    for child in node.children.iter().skip(1) {
                        written_specials(child, &mut written);
                    }
                }
            }
            written.retain(|(name, _)| !rebound.contains(name));
            // Keyed on the name, not the pair: `(rotatef *a* *a*)` names one
            // shared cell twice and is one finding, at the first reference.
            written.dedup_by(|left, right| left.0 == right.0);
            for (variable, span) in written {
                violations.push(UnsynchronizedSharedMutationItem { span, variable });
            }
        },
    );
}

/// Collects every unsynchronized global write inside a thread body in one file,
/// with the number of `make-thread` forms scanned as the denominator beside
/// them.
pub fn build_unsynchronized_shared_mutation_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<UnsynchronizedSharedMutationItem>> {
    let mut spawn_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_spawn(view, &mut spawn_count, &mut violations);
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

    fn report(input: &str) -> FileFindings<UnsynchronizedSharedMutationItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_unsynchronized_shared_mutation_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<UnsynchronizedSharedMutationItem> {
        report(input).findings
    }

    fn variables(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.variable)
            .collect()
    }

    #[test]
    fn flags_an_unlocked_increment_of_a_global_on_a_new_thread() {
        assert_eq!(
            variables("(bt:make-thread (lambda () (incf *counter*)))"),
            vec!["*counter*".to_owned()]
        );
    }

    #[test]
    fn flags_setf_setq_push_and_pop_of_a_global() {
        assert_eq!(
            variables("(make-thread (lambda () (setf *state* :done)))"),
            vec!["*state*".to_owned()]
        );
        assert_eq!(
            variables("(make-thread (lambda () (setq *state* :done)))"),
            vec!["*state*".to_owned()]
        );
        assert_eq!(
            variables("(make-thread (lambda () (push item *queue*)))"),
            vec!["*queue*".to_owned()]
        );
        assert_eq!(
            variables("(make-thread (lambda () (pop *queue*)))"),
            vec!["*queue*".to_owned()]
        );
    }

    #[test]
    fn flags_a_write_through_a_composite_place() {
        assert_eq!(
            variables("(make-thread (lambda () (setf (gethash key *table*) 1)))"),
            vec!["*table*".to_owned()]
        );
    }

    #[test]
    fn flags_a_write_nested_deep_inside_the_thunk() {
        assert_eq!(
            variables("(make-thread (lambda () (dolist (x xs) (when (ready-p x) (incf *n*)))))"),
            vec!["*n*".to_owned()]
        );
    }

    // --- realistic, correct concurrent code that must stay silent -----------

    #[test]
    fn does_not_flag_a_write_under_a_lock_scope() {
        assert!(
            violations(
                "(bt:make-thread\n\
                 \x20 (lambda ()\n\
                 \x20   (bt:with-lock-held (*counter-lock*)\n\
                 \x20     (incf *counter*))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_write_under_any_of_the_lock_macros() {
        for scope in [
            "(sb-thread:with-mutex (*m*) (incf *counter*))",
            "(bt:with-recursive-lock-held (*m*) (incf *counter*))",
            "(sb-thread:with-recursive-lock (*m*) (incf *counter*))",
            "(sb-ext:with-locked-hash-table (*table*) (setf (gethash k *table*) 1))",
        ] {
            let source = format!("(make-thread (lambda () {scope}))");
            assert!(violations(&source).is_empty(), "{scope} should be silent");
        }
    }

    /// The manual-lock idiom the sibling rule `lock-acquired-not-released`
    /// explicitly endorses. Calling the write inside it a race would make the
    /// package contradict itself.
    #[test]
    fn does_not_flag_a_write_under_a_hand_held_lock_with_unwind_protect() {
        assert!(
            violations(
                "(bt:make-thread\n\
                 \x20 (lambda ()\n\
                 \x20   (bt:acquire-lock *counter-lock*)\n\
                 \x20   (unwind-protect (incf *counter*)\n\
                 \x20     (bt:release-lock *counter-lock*))))"
            )
            .is_empty()
        );
    }

    /// Every real codebase wraps its lock in a local macro. The library's own
    /// spelling is not the only one that protects.
    #[test]
    fn does_not_flag_a_write_under_a_project_local_with_macro() {
        assert!(
            violations("(bt:make-thread (lambda () (with-registry-lock (push e *registry*))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_special_rebound_by_progv() {
        assert!(
            violations("(make-thread (lambda () (progv '(*depth*) '(0) (incf *depth*))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_special_rebound_as_a_lambda_parameter() {
        assert!(
            violations("(make-thread (lambda (*connection*) (setf *connection* nil)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_special_the_thunk_rebinds_for_itself() {
        assert!(
            violations("(make-thread (lambda () (let ((*depth* 0)) (incf *depth*))))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_write_to_a_local_variable() {
        assert!(violations("(make-thread (lambda () (let ((n 0)) (incf n))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_read_of_a_global() {
        assert!(violations("(make-thread (lambda () (log-to *sink* (compute))))").is_empty());
    }

    /// `(push *item* stack)` writes `stack` and only reads `*item*`. Scanning
    /// every argument of a mutator — which is what the neighbouring
    /// `global-mutation-in-function` does — would call this a race.
    #[test]
    fn does_not_flag_a_global_that_is_only_the_value_being_pushed() {
        assert!(violations("(make-thread (lambda () (push *item* local-stack)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_global_in_a_setf_value_position() {
        assert!(violations("(make-thread (lambda () (setf local *global-default*)))").is_empty());
    }

    #[test]
    fn does_not_flag_an_atomic_operation() {
        assert!(violations("(make-thread (lambda () (sb-ext:atomic-incf *counter*)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_thread_whose_function_is_named_rather_than_written() {
        assert!(violations("(make-thread #'worker)").is_empty());
        assert!(violations("(make-thread 'worker)").is_empty());
    }

    /// A computed thunk's *arguments* run on the spawning thread, not the new
    /// one. Without the literal-`lambda` requirement this write would be
    /// attributed to a thread that never performs it.
    #[test]
    fn does_not_flag_a_write_in_a_computed_thunks_arguments() {
        assert!(
            violations("(make-thread (build-worker (incf *n*)))").is_empty(),
            "(incf *n*) is evaluated by the caller, before any thread exists"
        );
    }

    #[test]
    fn does_not_flag_a_global_write_outside_any_thread() {
        assert!(violations("(defun reset () (setf *counter* 0))").is_empty());
    }

    #[test]
    fn does_not_flag_the_repl_history_variables() {
        assert!(violations("(make-thread (lambda () (setf ** nil)))").is_empty());
    }

    #[test]
    fn a_nested_spawn_is_not_charged_to_the_outer_one() {
        // The inner write belongs to the inner spawn, which is its own
        // candidate; it must be reported exactly once, not twice.
        assert_eq!(
            variables("(make-thread (lambda () (make-thread (lambda () (incf *n*)))))"),
            vec!["*n*".to_owned()]
        );
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(make-thread (lambda () (incf *counter*)))").is_empty());
        assert!(violations("(quote (make-thread (lambda () (incf *counter*))))").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_a_literal_comma_and_stays_data() {
        assert!(violations("'(a ,(make-thread (lambda () (incf *counter*))))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(make-thread (lambda () (incf *counter*)))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            variables("`(progn ,(make-thread (lambda () (incf *counter*))))"),
            vec!["*counter*".to_owned()]
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(format t \"(make-thread (lambda () (incf *counter*)))\")").is_empty());
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn case_folds_and_looks_through_the_package_qualifier() {
        assert_eq!(
            variables("(SB-THREAD:MAKE-THREAD (LAMBDA () (INCF *COUNTER*)))"),
            vec!["*counter*".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_spawn_scanned_not_only_the_flagged_ones() {
        let report = report("(make-thread #'worker)\n(make-thread (lambda () (incf *n*)))\n");
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_variable() {
        let report = report("(defun start ()\n  (make-thread (lambda () (incf *counter*))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "unsynchronized-shared-mutation");
        assert_eq!(
            finding.json_fields(),
            vec![("variable", json!("*counter*"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["variable=*counter*".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "*counter* is written on a new thread with no lock held, so concurrent writes race"
        );
    }

    /// The span points at the written variable — not at the `make-thread`,
    /// which may be many lines away, and not at the enclosing write form,
    /// which is the span `lint-safety`'s `global-mutation-in-function` reports
    /// for the very same line. Keeping the two spans distinct is the point:
    /// with both at `(setf *counter* …)` a reader sees two findings stacked on
    /// one range and reads them as a duplicate.
    #[test]
    fn the_finding_span_is_the_written_variable_not_the_form_or_the_thread() {
        let source = "(make-thread (lambda () (incf *counter*)))";
        let found = violations(source);
        let span = found[0].span;
        let text = &source[span.start().get()..span.end().get()];
        assert_eq!(text, "*counter*");
    }

    /// A composite place points at the shared cell inside it, not at the whole
    /// accessor call.
    #[test]
    fn a_composite_place_spans_the_table_not_the_accessor() {
        let source = "(make-thread (lambda () (setf (gethash key *table*) 1)))";
        let found = violations(source);
        let span = found[0].span;
        assert_eq!(&source[span.start().get()..span.end().get()], "*table*");
    }

    /// Two places in one `setf` used to land on one span; now each names its
    /// own variable.
    #[test]
    fn two_places_in_one_setf_get_two_distinct_spans() {
        let source = "(make-thread (lambda () (setf *a* 1 *b* 2)))";
        let found = violations(source);
        assert_eq!(found.len(), 2);
        assert_ne!(found[0].span, found[1].span);
        assert_eq!(
            &source[found[0].span.start().get()..found[0].span.end().get()],
            "*a*"
        );
        assert_eq!(
            &source[found[1].span.start().get()..found[1].span.end().get()],
            "*b*"
        );
    }

    /// One shared cell named twice in one form is one finding, at the first
    /// reference — the `dedup_by` keyed on the name rather than on the pair.
    #[test]
    fn one_variable_named_twice_in_one_form_is_reported_once() {
        let source = "(make-thread (lambda () (rotatef *a* *a*)))";
        let found = violations(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].variable, "*a*");
        assert_eq!(
            found[0].span.start().get(),
            source.find("*a*").expect("ref")
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(make-thread (lambda () (incf *counter*)))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_unsynchronized_shared_mutation_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(0))]);
    }
}
