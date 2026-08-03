//! `janet-empty-loop-body` detection: a Janet loop macro with a head and no
//! body.
//!
//! This is Janet's own lint, reproduced. `boot.janet` defines
//!
//! ```janet
//! (defn- check-empty-body
//!   [body]
//!   (if (= (length body) 0)
//!     (maclintf :normal "empty loop body")))
//! ```
//!
//! (`src/boot/boot.janet:626-629`) and calls it from `loop` (`:631`), `seq`
//! (`:709`) and `catseq` (`:717`). Those three heads are exactly what this rule
//! covers — not one more — because `maclintf` fires only at macro expansion
//! time, which means it reaches nobody who is reading a diff, reviewing a pull
//! request, or running a linter over a file they have not executed.
//!
//! `(loop [x :in xs])` iterates and does nothing: the head's `:when`/`:let`
//! clauses still run, so it is not a syntax error, and the usual cause is a
//! body that was deleted or that ended up outside the parentheses.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionView, SyntaxTree};

use crate::support::{head_symbol, symbol_text};

pub const DIALECTS: [Dialect; 1] = [Dialect::Janet];

/// The three macros `check-empty-body` guards, and nothing else.
///
/// `tabseq` deliberately does not call it (`boot.janet:720-725`) — its
/// `key-body` is mandatory and its value body may legitimately be absent — and
/// `each`/`while`/`for` are C-level or template macros that never had the
/// check. Extending the list would be this rule's opinion rather than Janet's.
pub const HEADS: [&str; 3] = ["loop", "seq", "catseq"];

/// The one loop verb whose expression is the work.
///
/// `:iterate` "repeatedly evaluate and bind to the expression while it is
/// truthy" (`boot.janet:647-648`), so `(loop [_ :iterate (parser/produce p)])`
/// drains a parser and *means* to have no body. Janet's own
/// `check-empty-body` does not know that and warns anyway; both of the two
/// findings this rule produced over 241 third-party Janet files were this
/// idiom, in `janet-lang/janet`'s own `test/suite-parse.janet:180` and `:185`.
///
/// Excluding it is this rule departing from Janet's check, deliberately: a
/// warning whose every real-world instance is correct code is a warning
/// nobody keeps switched on.
const EFFECT_VERB: &str = ":iterate";

/// One loop macro whose body is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyLoopBody {
    pub span: ByteSpan,
    pub head: String,
}

/// Examines one form.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<EmptyLoopBody> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let head = head_symbol(view)?;
    if !HEADS.contains(&head) {
        return None;
    }
    // `(loop)` with no head at all is malformed and Janet's own
    // `check-empty-body` never runs on it; the compiler's error is the better
    // message.
    let binder = view.children.get(1)?;
    if binder.delimiter != Some(Delimiter::Bracket) {
        return None;
    }
    let drains = binder
        .children
        .iter()
        .any(|child| symbol_text(child) == Some(EFFECT_VERB));
    if drains {
        return None;
    }
    // Head plus binder is two children; a body is anything after that.
    (view.children.len() == 2).then(|| EmptyLoopBody {
        span: view.span,
        head: head.to_owned(),
    })
}

/// Every empty-bodied loop macro in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<EmptyLoopBody> {
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

/// Every `loop`/`seq`/`catseq` form in the file, empty or not. The denominator.
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

    fn heads(source: &str, dialect: Dialect) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        collect(dialect, &tree)
            .into_iter()
            .map(|item| item.head)
            .collect()
    }

    #[test]
    fn flags_each_of_the_three_macros_janet_itself_guards() {
        assert_eq!(heads("(loop [x :in xs])", Dialect::Janet), vec!["loop"]);
        assert_eq!(heads("(seq [x :in xs])", Dialect::Janet), vec!["seq"]);
        assert_eq!(heads("(catseq [x :in xs])", Dialect::Janet), vec!["catseq"]);
    }

    #[test]
    fn a_loop_with_a_body_is_left_alone() {
        assert!(heads("(loop [x :in xs] (print x))", Dialect::Janet).is_empty());
        assert!(heads("(seq [x :in xs] x)", Dialect::Janet).is_empty());
    }

    #[test]
    fn a_body_of_nil_still_counts_as_a_body() {
        // Writing `nil` is a deliberate statement; an absent body is not.
        assert!(heads("(loop [x :in xs] nil)", Dialect::Janet).is_empty());
    }

    #[test]
    fn a_multi_clause_head_does_not_count_as_a_body() {
        assert_eq!(
            heads("(loop [i :range [0 10] :when (even? i)])", Dialect::Janet),
            vec!["loop"]
        );
    }

    #[test]
    fn the_iterate_drain_idiom_is_not_reported() {
        // janet-lang/janet test/suite-parse.janet:180.
        assert!(heads("(loop [_ :iterate (parser/produce p1)])", Dialect::Janet).is_empty());
        // The control: the same shape with any other verb still reports, so
        // the exclusion is the verb and not the underscore or the call.
        assert_eq!(
            heads("(loop [_ :in (parser/produce p1)])", Dialect::Janet),
            vec!["loop"]
        );
    }

    #[test]
    fn a_malformed_loop_is_left_to_the_compiler() {
        assert!(heads("(loop)", Dialect::Janet).is_empty());
        assert!(heads("(loop x)", Dialect::Janet).is_empty());
    }

    #[test]
    fn heads_outside_janets_own_list_are_not_covered() {
        // `each`, `while` and `for` have no `check-empty-body` call.
        assert!(heads("(each x xs)", Dialect::Janet).is_empty());
        assert!(heads("(while true)", Dialect::Janet).is_empty());
        assert!(heads("(tabseq [x :in xs] x)", Dialect::Janet).is_empty());
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // Common Lisp's `loop` with only clauses is complete code. Its reader
        // has no bracket delimiter, so the clause list is written `(…)`.
        assert!(heads("(loop for x in xs count x)", Dialect::CommonLisp).is_empty());
        // Fennel has no `loop` special at all; this is a call to a function
        // named `loop` with one sequence argument.
        assert!(heads("(loop [x :in xs])", Dialect::Fennel).is_empty());
    }

    #[test]
    fn the_candidate_count_counts_every_loop_macro() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop [x :in a] (f x)) (loop [x :in b])",
            Dialect::Janet,
        )
        .expect("parse");
        assert_eq!(candidate_count(Dialect::Janet, &tree), 2);
        assert_eq!(collect(Dialect::Janet, &tree).len(), 1);
    }
}
