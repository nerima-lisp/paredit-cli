//! `janet-dead-branch-on-constant-condition` detection: a Janet conditional
//! whose test is a literal, so one of its branches can never run.
//!
//! # Primary source
//!
//! This is Janet's own compiler lint. `src/core/specials.c:675-695` folds a
//! constant condition and hands the losing branch to `janetc_throwaway`, which
//! reports it (`src/core/compile.c:590-597`):
//!
//! ```c
//! void janetc_throwaway(JanetFopts opts, Janet x) {
//!     …
//!     janetc_lintf(c, JANET_C_LINT_STRICT, "dead code, consider removing %.4q", x);
//! ```
//!
//! Confirmed by running the compiler on `janet 1.41.2` and reading the lint
//! array back, rather than by reading the C:
//!
//! ```janet
//! (def l @[])
//! (compile ~(if true :a :b) (fiber/getenv (fiber/current)) "t" l)
//! (pp l)  # @[(:strict nil nil "dead code, consider removing :b")]
//! ```
//!
//! The same probe establishes each of the shapes this rule models, and each of
//! the ones it must not:
//!
//! ```text
//! (if true :a :b)      -> dead code, consider removing :b
//! (if false :a :b)     -> dead code, consider removing :a
//! (if nil :a :b)       -> dead code, consider removing :a
//! (if 1 :a :b)         -> dead code, consider removing :b
//! (if "s" :a :b)       -> dead code, consider removing :b
//! (if :kw :a :b)       -> dead code, consider removing :b
//! (when false :a)      -> dead code, consider removing (do :a)
//! (unless true :a)     -> dead code, consider removing (do :a)
//! (if-not false :a :b) -> dead code, consider removing :b
//!
//! (if true :a)         -> no lint      ; the absent branch is nil
//! (if true :a nil)     -> no lint      ; an explicit nil branch is skipped
//! (if @[] :a :b)       -> no lint      ; a mutable literal is not constant
//! (if (= 1 1) :a :b)   -> no lint      ; calls are not folded
//! (when true :a)       -> no lint      ; nothing is dead
//! ```
//!
//! # What this rule deliberately cannot see
//!
//! Janet folds a `def`-bound value into a constant too, so `(def x 5)` followed
//! by `(if x :a :b)` lints in the real compiler — the probe above reproduces it
//! by binding `realvar` in the environment. That needs the binding's *value*,
//! and [`paredit_core_lint_engine::engine::RuleContext::value_table`] is empty
//! for Janet, so this rule sees only literals written in place. Every finding
//! it does produce is a finding the compiler would also produce; the converse
//! does not hold.
//!
//! Truthiness is Janet's: `nil` and `false` are false and *everything* else is
//! true, so `0` and `""` are truthy conditions.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};

use crate::support::{head_symbol, symbol_text};

pub const DIALECTS: [Dialect; 1] = [Dialect::Janet];

/// The four conditional heads whose constant-folded branch the compiler
/// reports.
///
/// `when` and `unless` are macros over `if` (`boot.janet`), so the lint
/// reaches them after expansion; they are listed here because this rule does
/// not expand macros and would otherwise miss them.
///
/// `cond`, `case` and `match` are absent on purpose: their dead-branch
/// analysis is about pattern reachability rather than a folded test, which is
/// `janet-unreachable-match-clause`'s job.
pub const HEADS: [&str; 4] = ["if", "if-not", "when", "unless"];

/// Which branch of a conditional cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadBranch {
    /// The `then` branch of an `if`/`if-not`.
    Then,
    /// The `else` branch of an `if`/`if-not`.
    Else,
    /// The whole body of a `when`/`unless`.
    Body,
}

impl DeadBranch {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Then => "the `then` branch",
            Self::Else => "the `else` branch",
            Self::Body => "the body",
        }
    }
}

/// One conditional with an unreachable branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantCondition {
    /// The dead code itself, or the whole form when the dead part is a
    /// multi-form body.
    pub span: ByteSpan,
    /// The whole conditional — the node the engine dispatched on, and so the
    /// span the rule's quote guard is asked about. See `fennel_bad_unpack`'s
    /// `form_span` for why the two spans can disagree.
    pub form_span: ByteSpan,
    pub head: String,
    /// The condition as written, so the message can quote it.
    pub condition: String,
    pub branch: DeadBranch,
}

/// Whether a literal is true, false, or not a compile-time constant at all.
///
/// `None` means "not decidable from the token", which is the answer for every
/// symbol, every call, and every mutable literal.
fn literal_truth(view: &ExpressionView) -> Option<bool> {
    // `@[…]`, `@{…}` and `@"…"` are freshly allocated on every evaluation, so
    // the compiler does not fold them — verified: `(if @[] :a :b)` lints not
    // at all. The reader records the `@` as `HashLiteral`.
    if view.reader_prefixes.contains(&ReaderPrefix::HashLiteral) {
        return None;
    }
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = symbol_text(view)?;
    match text {
        "nil" | "false" => Some(false),
        "true" => Some(true),
        // Janet truthiness: everything that is not nil or false is true. A
        // keyword, a string (including a `` ` `` long string) and a number are
        // all constants, and all truthy.
        _ if text.starts_with(':') || text.starts_with('"') || text.starts_with('`') => Some(true),
        _ if is_number_literal(text) => Some(true),
        _ => None,
    }
}

/// Whether `text` is a Janet number literal rather than a symbol.
///
/// Deliberately conservative: it must start with a digit, or with a sign or dot
/// followed by a digit. Janet symbols may contain digits but may not begin
/// like this, so a `false` answer never loses a real constant that matters and
/// a `true` answer is never a symbol.
fn is_number_literal(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if first.is_ascii_digit() {
        return true;
    }
    if matches!(first, b'-' | b'+' | b'.') {
        return bytes.next().is_some_and(|byte| byte.is_ascii_digit());
    }
    false
}

/// Whether a branch node is worth reporting.
///
/// An explicit `nil` branch is skipped, matching the compiler:
/// `if (!janet_checktype(falsebody, JANET_NIL)) janetc_throwaway(…)`
/// (`specials.c:692`). `(if true :a nil)` produces no lint, and neither does
/// this.
fn is_reportable_branch(view: &ExpressionView) -> bool {
    symbol_text(view) != Some("nil")
}

/// Examines one form.
///
/// Cheap: at most one slice `contains`, one child lookup and one token
/// inspection before it declines.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<ConstantCondition> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let head = head_symbol(view)?;
    if !HEADS.contains(&head) {
        return None;
    }
    let condition = view.children.get(1)?;
    let raw = literal_truth(condition)?;
    // `if-not` and `unless` invert the test before the branches are chosen.
    let taken = match head {
        "if-not" | "unless" => !raw,
        _ => raw,
    };
    let condition_text = symbol_text(condition)?.to_owned();

    let (span, branch) = match head {
        "if" | "if-not" => {
            // (if cond then else?) — nothing else is well formed, and the
            // compiler's own error is the better message for anything longer.
            if view.children.len() < 3 || view.children.len() > 4 {
                return None;
            }
            let index = if taken { 3 } else { 2 };
            let dead = view.children.get(index)?;
            if !is_reportable_branch(dead) {
                return None;
            }
            (
                dead.span,
                if taken {
                    DeadBranch::Else
                } else {
                    DeadBranch::Then
                },
            )
        }
        // (when cond body…) runs the body only when the test holds, so a body
        // is dead exactly when the test is false. `unless` already inverted
        // `taken` above, so both read the same way here.
        _ => {
            if taken || view.children.len() < 3 {
                return None;
            }
            (view.span, DeadBranch::Body)
        }
    };

    Some(ConstantCondition {
        span,
        form_span: view.span,
        head: head.to_owned(),
        condition: condition_text,
        branch,
    })
}

/// Every conditional with a dead branch in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<ConstantCondition> {
    let root = tree.root_view();
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// Every `if`/`if-not`/`when`/`unless` in the file. The denominator.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| HEADS.contains(&head)) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source: &str) -> Vec<ConstantCondition> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Janet).expect("parse");
        collect(Dialect::Janet, &tree)
    }

    fn branches(source: &str) -> Vec<DeadBranch> {
        found(source).into_iter().map(|item| item.branch).collect()
    }

    /// The exact table the `janet 1.41.2` probe produced, replayed.
    #[test]
    fn every_shape_the_real_compiler_lints_is_reported() {
        assert_eq!(branches("(if true :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(if false :a :b)"), vec![DeadBranch::Then]);
        assert_eq!(branches("(if nil :a :b)"), vec![DeadBranch::Then]);
        assert_eq!(branches("(if 1 :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(if \"s\" :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(if :kw :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(when false :a)"), vec![DeadBranch::Body]);
        assert_eq!(branches("(unless true :a)"), vec![DeadBranch::Body]);
        assert_eq!(branches("(if-not false :a :b)"), vec![DeadBranch::Else]);
    }

    /// The other half of the same table: what it must *not* report.
    #[test]
    fn every_shape_the_real_compiler_leaves_alone_is_left_alone() {
        assert!(branches("(if true :a)").is_empty(), "absent else branch");
        assert!(branches("(if true :a nil)").is_empty(), "explicit nil");
        assert!(branches("(if @[] :a :b)").is_empty(), "mutable literal");
        assert!(branches("(if @{} :a :b)").is_empty(), "mutable literal");
        // The buffer is the one that matters. `@[]` and `@{}` are *lists* to
        // the reader and would be rejected by the `ExpressionKind::Atom` check
        // even without the `@` guard, so only `@""` — an atom whose `@` the
        // prefix stripping would otherwise discard, leaving a plain string —
        // can tell the two apart. Mutation testing found the guard unkilled
        // until this line existed. Verified against janet 1.41.2:
        // `(if @"" :a :b)` lints not at all, `(if "" :a :b)` lints.
        assert!(
            branches("(if @\"\" :a :b)").is_empty(),
            "a buffer is mutable"
        );
        assert_eq!(
            branches("(if \"\" :a :b)"),
            vec![DeadBranch::Else],
            "but a string is a constant"
        );
        assert!(branches("(if (= 1 1) :a :b)").is_empty(), "a call");
        assert!(branches("(when true :a)").is_empty(), "live body");
        assert!(branches("(unless false :a)").is_empty(), "live body");
    }

    #[test]
    fn a_symbol_condition_is_not_a_literal_here() {
        // The compiler folds a `def`-bound value and would lint this; without
        // a value table this rule cannot and must not guess.
        assert!(branches("(if flag :a :b)").is_empty());
        assert!(branches("(if my-const :a :b)").is_empty());
    }

    #[test]
    fn zero_and_the_empty_string_are_truthy_in_janet() {
        // The single most likely way to get this backwards.
        assert_eq!(branches("(if 0 :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(if \"\" :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(when 0 :a)"), Vec::<DeadBranch>::new());
    }

    #[test]
    fn a_number_is_told_apart_from_a_symbol_that_contains_digits() {
        assert_eq!(branches("(if -1 :a :b)"), vec![DeadBranch::Else]);
        assert_eq!(branches("(if 3.5 :a :b)"), vec![DeadBranch::Else]);
        assert!(branches("(if x1 :a :b)").is_empty());
        assert!(branches("(if -x :a :b)").is_empty());
        assert!(branches("(if + :a :b)").is_empty());
    }

    #[test]
    fn the_finding_points_at_the_dead_branch_for_an_if() {
        let source = "(if true :alive :dead)";
        let item = found(source).remove(0);
        assert_eq!(
            &source[item.span.start().get()..item.span.end().get()],
            ":dead"
        );
    }

    #[test]
    fn the_finding_points_at_the_whole_form_for_a_when() {
        let source = "(when false (f) (g))";
        let item = found(source).remove(0);
        assert_eq!(
            &source[item.span.start().get()..item.span.end().get()],
            source
        );
    }

    #[test]
    fn a_bodyless_when_has_nothing_dead_to_report() {
        assert!(branches("(when false)").is_empty());
        assert!(branches("(unless true)").is_empty());
    }

    #[test]
    fn a_malformed_if_is_left_to_the_compiler() {
        assert!(branches("(if)").is_empty());
        assert!(branches("(if true)").is_empty());
        assert!(branches("(if true :a :b :c)").is_empty());
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // In Fennel `nil` is also falsy but `if-not` and `unless` do not exist,
        // and Common Lisp's `nil`/`t` are a different vocabulary entirely.
        for dialect in [Dialect::Fennel, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect("(if true :a :b)", dialect).expect("parse");
            assert!(collect(dialect, &tree).is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn the_candidate_count_counts_every_conditional() {
        let tree = SyntaxTree::parse_with_dialect(
            "(if true :a :b) (if x :a :b) (when y (f)) (cond z (f))",
            Dialect::Janet,
        )
        .expect("parse");
        assert_eq!(candidate_count(Dialect::Janet, &tree), 3);
        assert_eq!(collect(Dialect::Janet, &tree).len(), 1);
    }
}
