//! `self-recursive-tail-call` detection: a function's own name called in tail
//! position of its own body, annotated with whether the target dialect
//! guarantees tail-call optimization (TCO) there.
//!
//! A self-recursive call in tail position is a loop written as a function
//! call — worthwhile only where the implementation is guaranteed to run it in
//! constant stack space. Whether that is true is a fact about the *dialect*,
//! not the code:
//!
//! - **Scheme** and **Racket** require proper tail calls by specification —
//!   this pattern is always safe.
//! - **LFE** compiles to BEAM bytecode, which performs Erlang's last-call
//!   optimization — also always safe.
//! - **Fennel** compiles to Lua, whose reference manual mandates proper tail
//!   calls for a `return f(...)` — also always safe.
//! - **Common Lisp** does not require TCO in the standard; whether this
//!   pattern is safe depends on the implementation and its optimization
//!   settings.
//! - **Emacs Lisp** and **Hy** (which compiles to Python) have no TCO at
//!   all — this pattern will grow the stack on every call and can exhaust it
//!   on deep recursion.
//! - **Clojure** does not perform automatic TCO either; the sanctioned
//!   replacement is `recur`, not a plain recursive call.
//! - **Janet** and **Carp** are not modeled: this rule does not assert a fact
//!   about their tail-call behaviour it cannot verify.
//!
//! Tail position is computed structurally: the last form of a body/`progn`/
//! `let`/`when`/`unless`, both branches of an `if`, the last form of each
//! `cond`/`case`-family clause, and the last operand of `and`/`or`. Anything
//! inside a call's *arguments* is never tail position, including the
//! arguments of the self-recursive call itself.
//!
//! Report-only, and the finding's severity is informational rather than a
//! defect: a self-recursive tail call in a TCO-guaranteed dialect is not a
//! problem to fix, it is the reader being told the compiler will do the right
//! thing with it.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in, unqualified};

#[derive(Debug, Clone)]
pub struct SelfRecursiveTailCallItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
    pub tco_status: TcoStatus,
}

/// What this rule knows about the target dialect's tail-call behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcoStatus {
    /// The dialect's specification (or the platform it compiles to)
    /// guarantees this call runs in constant stack space.
    Guaranteed,
    /// The dialect performs no tail-call optimization at all.
    NotPerformed,
    /// The standard does not require it; whether it happens depends on the
    /// implementation.
    ImplementationDefined,
    /// This rule does not assert a fact about this dialect's behaviour.
    NotModeled,
}

impl TcoStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Guaranteed => "guaranteed",
            Self::NotPerformed => "not-performed",
            Self::ImplementationDefined => "implementation-defined",
            Self::NotModeled => "not-modeled",
        }
    }

    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::Guaranteed => {
                "this dialect guarantees a proper tail call here, so this runs in constant stack space"
            }
            Self::NotPerformed => {
                "this dialect performs no tail-call optimization; this call grows the stack every time"
            }
            Self::ImplementationDefined => {
                "the standard does not require tail-call optimization here; whether this runs in \
                 constant stack space depends on the implementation"
            }
            Self::NotModeled => {
                "this rule does not assert a fact about this dialect's tail-call behaviour"
            }
        }
    }
}

/// What this rule knows about `dialect`'s tail-call guarantees.
#[must_use]
pub const fn tco_status(dialect: Dialect) -> TcoStatus {
    match dialect {
        Dialect::Scheme | Dialect::Racket | Dialect::Lfe | Dialect::Fennel => TcoStatus::Guaranteed,
        Dialect::EmacsLisp | Dialect::Hy | Dialect::Clojure => TcoStatus::NotPerformed,
        Dialect::CommonLisp => TcoStatus::ImplementationDefined,
        Dialect::Carp | Dialect::Janet | Dialect::Unknown => TcoStatus::NotModeled,
    }
}

#[derive(Debug)]
pub struct SelfRecursiveTailCallSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<SelfRecursiveTailCallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SelfRecursiveTailCallPolicyOptions {
    fail_on_violation: bool,
}

impl SelfRecursiveTailCallPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct SelfRecursiveTailCallPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Every tail-position sub-form of `view`, treated as a body/`progn`-like
/// sequence: the whole of `view` when it is not one of the special forms
/// below, or the recursively-computed tail positions of the arm(s) that flow
/// to `view`'s own result.
fn collect_tail_positions<'a>(view: &'a ExpressionView, out: &mut Vec<&'a ExpressionView>) {
    let Some(head) = list_head(view) else {
        out.push(view);
        return;
    };

    if symbol_in(head, &["progn", "locally", "eval-when", "prog1", "prog2"]) {
        // `prog1`/`prog2` return an *earlier* form's value, not the last —
        // nothing inside either is in tail position relative to the whole
        // form, so neither recurses further.
        if symbol_in(head, &["prog1", "prog2"]) {
            return;
        }
        if let Some(last) = view.children.last() {
            collect_tail_positions(last, out);
        }
        return;
    }

    if symbol_in(head, &["let", "let*", "flet", "labels", "macrolet"]) {
        // children[0] = head, children[1] = bindings, children[2..] = body.
        if let Some(last) = view.children.iter().skip(2).next_back() {
            collect_tail_positions(last, out);
        }
        return;
    }

    if symbol_in(head, &["when", "unless"]) {
        if let Some(last) = view.children.iter().skip(2).next_back() {
            collect_tail_positions(last, out);
        }
        return;
    }

    if symbol_in(head, &["if"]) {
        if let Some(then) = view.children.get(2) {
            collect_tail_positions(then, out);
        }
        if let Some(otherwise) = view.children.get(3) {
            collect_tail_positions(otherwise, out);
        }
        return;
    }

    if symbol_in(head, &["cond"]) {
        for clause in view.children.iter().skip(1) {
            if let Some(last) = clause.children.last() {
                collect_tail_positions(last, out);
            }
        }
        return;
    }

    if symbol_in(
        head,
        &[
            "case",
            "ecase",
            "ccase",
            "typecase",
            "etypecase",
            "ctypecase",
        ],
    ) {
        for clause in view.children.iter().skip(2) {
            if let Some(last) = clause.children.last() {
                collect_tail_positions(last, out);
            }
        }
        return;
    }

    if symbol_in(head, &["and", "or"]) {
        if let Some(last) = view.children.iter().skip(1).next_back() {
            collect_tail_positions(last, out);
        }
        return;
    }

    // An ordinary call, or a special form this rule does not model the
    // control flow of: the whole form is the tail position, and nothing
    // inside its arguments is (a self-recursive call is never buried inside
    // another call's arguments and still in tail position).
    out.push(view);
}

/// Examines one node: if it is a function-shaped definition, finds every
/// self-recursive call in the tail position of its body.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    dialect: Dialect,
    scanned_form_count: &mut usize,
    violations: &mut Vec<SelfRecursiveTailCallItem>,
) {
    *scanned_form_count += 1;
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(shape) = definition_shape(dialect, view, head) else {
        return;
    };
    if shape.category != DefinitionCategory::Function {
        return;
    }
    let Some(name) = shape.name(view) else {
        return;
    };
    let lowered_name = unqualified(name).to_ascii_lowercase();

    let body = shape.body_forms(view);
    let Some(last) = body.last() else {
        return;
    };

    let mut tail_positions = Vec::new();
    collect_tail_positions(last, &mut tail_positions);

    for tail in tail_positions {
        let Some(call_head) = list_head(tail) else {
            continue;
        };
        if unqualified(call_head).to_ascii_lowercase() == lowered_name {
            violations.push(SelfRecursiveTailCallItem {
                path: path.to_path_buf(),
                span: tail.span,
                name: name.to_owned(),
                tco_status: tco_status(dialect),
            });
        }
    }
}

/// Collects every violation across a whole file, along with the total number of
/// forms scanned.
pub fn collect_self_recursive_tail_call(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<SelfRecursiveTailCallItem>)> {
    let mut scanned_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        paredit_core_syntax::view_query::for_each_subview(&view, |subview| {
            examine(
                subview,
                path,
                dialect,
                &mut scanned_form_count,
                &mut violations,
            );
        });
    }
    Ok((scanned_form_count, violations))
}

#[must_use]
pub const fn summarize_self_recursive_tail_call(
    scanned_form_count: usize,
    violations: Vec<SelfRecursiveTailCallItem>,
) -> SelfRecursiveTailCallSummary {
    SelfRecursiveTailCallSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_self_recursive_tail_call_policy(
    options: SelfRecursiveTailCallPolicyOptions,
    summary: &SelfRecursiveTailCallSummary,
) -> SelfRecursiveTailCallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SelfRecursiveTailCallPolicy {
        fail_on_violation: options.fail_on_violation(),
        scanned_form_count: summary.scanned_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(input: &str, dialect: Dialect) -> Vec<SelfRecursiveTailCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_self_recursive_tail_call(&PathBuf::from("t.lisp"), dialect, &tree)
                .expect("collect");
        violations
    }

    #[test]
    fn flags_a_direct_tail_call_common_lisp() {
        let found = findings(
            "(defun count-down (n) (if (zerop n) 'done (count-down (1- n))))",
            Dialect::CommonLisp,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "count-down");
        assert_eq!(found[0].tco_status, TcoStatus::ImplementationDefined);
    }

    #[test]
    fn flags_a_tail_call_through_cond_and_when() {
        let found = findings(
            "(defun f (n acc) (cond ((zerop n) acc) (t (when t (f (1- n) acc)))))",
            Dialect::CommonLisp,
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn does_not_flag_a_call_buried_in_an_argument() {
        assert!(
            findings(
                "(defun f (n) (if (zerop n) 0 (+ 1 (f (1- n)))))",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_call_to_another_function() {
        assert!(
            findings(
                "(defun f (n) (if (zerop n) 0 (g (1- n))))",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }

    #[test]
    fn reports_guaranteed_tco_for_scheme() {
        let found = findings(
            "(define (loop n) (if (zero? n) 'done (loop (- n 1))))",
            Dialect::Scheme,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].tco_status, TcoStatus::Guaranteed);
    }

    #[test]
    fn reports_not_performed_for_clojure() {
        let found = findings(
            "(defn loop2 [n] (if (zero? n) :done (loop2 (dec n))))",
            Dialect::Clojure,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].tco_status, TcoStatus::NotPerformed);
    }

    #[test]
    fn does_not_flag_a_call_inside_prog1() {
        // prog1 returns its *first* form's value; nothing inside it is tail.
        assert!(
            findings(
                "(defun f (n) (prog1 (f (1- n)) (report n)))",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }
}
