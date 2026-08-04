//! Which `except` clause of a Hy `try` an earlier clause already covers.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::support::{hy_atom, hy_head, is_bracket_list};

/// Hy only. `try`/`except` compiles to a Python `Try` node, and the defect is
/// Python's rule that the first matching handler wins.
pub const DIALECTS: [Dialect; 1] = [Dialect::Hy];

/// The head this rule anchors on.
///
/// `try` rather than `except`, and that is the whole design: reachability is a
/// property of a clause's *position among its siblings*, which a rule handed
/// one `except` at a time cannot see. The sibling package's `hy-bare-except`
/// anchors on `except` because breadth is a property of one clause alone.
pub const HEADS: [&str; 1] = ["try"];

/// What one `except` clause catches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caught<'a> {
    /// A catch-everything: `(except [] …)`, Python's bare `except:`.
    Everything,
    /// The named exception types, borrowed from the source rather than
    /// copied. Allocating a `String` per type name showed up directly in the
    /// cost measurement: this type is built once per `except` clause of every
    /// multi-clause `try` in the file, on the clean path where nothing is
    /// reported.
    Types(Vec<&'a str>),
    /// A shape this layer cannot read — a computed type, or a binding list
    /// that is not the documented shape. Never reported and never used to
    /// report anything else.
    Unknown,
}

/// Python's builtin exception hierarchy, as `(subtype, supertype)` edges.
///
/// Transitivity is computed rather than listed, so one edge extends every
/// chain through it. This carries only what the language itself fixes: a
/// project's own exception classes have a hierarchy this layer cannot see, and
/// [`is_subtype_of`] declines every question about them.
const HIERARCHY: [(&str, &str); 55] = [
    ("Exception", "BaseException"),
    ("GeneratorExit", "BaseException"),
    ("KeyboardInterrupt", "BaseException"),
    ("SystemExit", "BaseException"),
    ("ArithmeticError", "Exception"),
    ("AssertionError", "Exception"),
    ("AttributeError", "Exception"),
    ("BufferError", "Exception"),
    ("EOFError", "Exception"),
    ("ImportError", "Exception"),
    ("LookupError", "Exception"),
    ("MemoryError", "Exception"),
    ("NameError", "Exception"),
    ("OSError", "Exception"),
    ("ReferenceError", "Exception"),
    ("RuntimeError", "Exception"),
    ("StopAsyncIteration", "Exception"),
    ("StopIteration", "Exception"),
    ("SyntaxError", "Exception"),
    ("SystemError", "Exception"),
    ("TypeError", "Exception"),
    ("ValueError", "Exception"),
    ("Warning", "Exception"),
    ("FloatingPointError", "ArithmeticError"),
    ("OverflowError", "ArithmeticError"),
    ("ZeroDivisionError", "ArithmeticError"),
    ("IndexError", "LookupError"),
    ("KeyError", "LookupError"),
    ("ModuleNotFoundError", "ImportError"),
    ("UnboundLocalError", "NameError"),
    ("IndentationError", "SyntaxError"),
    ("TabError", "IndentationError"),
    ("NotImplementedError", "RuntimeError"),
    ("RecursionError", "RuntimeError"),
    ("UnicodeError", "ValueError"),
    ("UnicodeDecodeError", "UnicodeError"),
    ("UnicodeEncodeError", "UnicodeError"),
    ("UnicodeTranslateError", "UnicodeError"),
    ("BlockingIOError", "OSError"),
    ("ChildProcessError", "OSError"),
    ("ConnectionError", "OSError"),
    ("FileExistsError", "OSError"),
    ("FileNotFoundError", "OSError"),
    ("InterruptedError", "OSError"),
    ("IsADirectoryError", "OSError"),
    ("NotADirectoryError", "OSError"),
    ("PermissionError", "OSError"),
    ("ProcessLookupError", "OSError"),
    ("TimeoutError", "OSError"),
    ("BrokenPipeError", "ConnectionError"),
    ("ConnectionAbortedError", "ConnectionError"),
    ("ConnectionRefusedError", "ConnectionError"),
    ("ConnectionResetError", "ConnectionError"),
    ("BytesWarning", "Warning"),
    ("DeprecationWarning", "Warning"),
];

/// The builtin aliases that name a class already in [`HIERARCHY`].
///
/// These are not subclasses, they are the *same object*: Python binds
/// `IOError`, `EnvironmentError` and `WindowsError` to `OSError` itself.
/// Canonicalizing means `(except [e IOError] …)` followed by
/// `(except [e OSError] …)` is recognized as the duplicate it is, in both
/// directions — a subtype edge would only have caught one of them.
fn canonical(name: &str) -> &str {
    match name {
        "IOError" | "EnvironmentError" | "WindowsError" => "OSError",
        other => other,
    }
}

/// Whether `subtype` is `supertype`, or inherits from it.
///
/// Deliberately declines every question it cannot settle from the language's
/// own hierarchy. The one exception is the root: **every** catchable Python
/// exception derives from `BaseException`, so a clause naming it covers even a
/// class this layer has never heard of. `Exception` gets no such treatment —
/// a project's exception class may derive from `BaseException` directly, in
/// which case `except Exception` does not catch it and the later clause is
/// live. Under-reporting is the only safe direction here: telling somebody
/// their carefully written handler is dead, wrongly, is worse than silence.
#[must_use]
pub fn is_subtype_of(subtype: &str, supertype: &str) -> bool {
    let (subtype, supertype) = (canonical(subtype), canonical(supertype));
    if supertype == "BaseException" {
        return true;
    }
    if subtype == supertype {
        return true;
    }
    // Walk up from the subtype. The chain is short and acyclic, but the bound
    // makes that a property of this function rather than of the table.
    let mut current = subtype;
    for _ in 0..HIERARCHY.len() {
        let Some((_, parent)) = HIERARCHY.iter().find(|(child, _)| *child == current) else {
            return false;
        };
        if *parent == supertype {
            return true;
        }
        current = parent;
    }
    false
}

/// What the binding list of an `(except …)` clause says it catches.
///
/// Hy accepts four shapes, verified against Hy 1.3.1 by the sibling package
/// and re-derived here from the parse:
///
/// ```text
/// (except []                        …)   bare: Python `except:`
/// (except [ValueError]              …)   one type, no bound name
/// (except [e ValueError]            …)   bound name and type
/// (except [e [ValueError KeyError]] …)   bound name and a tuple of types
/// ```
///
/// A one-element list holding a *list* — `[[ValueError KeyError]]` — is a
/// tuple of types with no bound name, which is why the arity alone does not
/// decide which element is the type.
#[must_use]
pub fn caught_by<'a>(clause: &'a ExpressionView) -> Caught<'a> {
    let Some(bindings) = clause.children.get(1) else {
        return Caught::Unknown;
    };
    if !is_bracket_list(bindings) {
        return Caught::Unknown;
    }
    match bindings.children.len() {
        0 => Caught::Everything,
        1 => type_names(&bindings.children[0]),
        2 => type_names(&bindings.children[1]),
        _ => Caught::Unknown,
    }
}

/// The type names a single type position denotes: one atom, or a bracket list
/// of atoms.
fn type_names<'a>(view: &'a ExpressionView) -> Caught<'a> {
    if let Some(name) = hy_atom(view) {
        // `BaseException` in the type position catches everything, so it is
        // the same verdict as a bare `except`.
        if canonical(name) == "BaseException" {
            return Caught::Everything;
        }
        return Caught::Types(vec![name]);
    }
    if !is_bracket_list(view) {
        return Caught::Unknown;
    }
    let mut names = Vec::with_capacity(view.children.len());
    for child in &view.children {
        let Some(name) = hy_atom(child) else {
            // One unreadable element makes the whole tuple unreadable: the
            // clause catches something this layer cannot name, so no later
            // clause may be called dead on its account.
            return Caught::Unknown;
        };
        if canonical(name) == "BaseException" {
            return Caught::Everything;
        }
        names.push(name);
    }
    if names.is_empty() {
        return Caught::Unknown;
    }
    Caught::Types(names)
}

/// Whether `earlier` already catches everything `later` would.
#[must_use]
pub fn covers(earlier: &Caught<'_>, later: &Caught<'_>) -> bool {
    match (earlier, later) {
        (Caught::Unknown, _) | (_, Caught::Unknown) => false,
        (Caught::Everything, _) => true,
        (Caught::Types(_), Caught::Everything) => false,
        (Caught::Types(before), Caught::Types(after)) => after
            .iter()
            .all(|name| before.iter().any(|seen| is_subtype_of(name, seen))),
    }
}

/// One dead clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadClause {
    /// The span of the unreachable `(except …)` clause.
    pub span: ByteSpan,
    /// The one-based position of the clause among the `try`'s except clauses,
    /// and of the earlier clause that shadows it.
    pub position: usize,
    pub shadowed_by: usize,
    /// What the shadowing clause catches, for the message.
    pub reason: Shadow,
}

/// Why a clause is dead, which decides how the finding is phrased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shadow {
    /// An earlier clause catches every exception there is.
    CatchAll,
    /// An earlier clause names the same type.
    SameType(String),
    /// An earlier clause names a supertype.
    Supertype(String),
}

/// The `except` clauses of a `try`, in source order.
///
/// `else` and `finally` are siblings of the `except` clauses and are skipped;
/// so is the body, which is every form before the first clause.
fn except_clauses(form: &ExpressionView) -> Vec<&ExpressionView> {
    form.children
        .iter()
        .skip(1)
        .filter(|child| hy_head(child) == Some("except"))
        .collect()
}

/// Every clause of `form` that an earlier clause makes unreachable.
///
/// `form` must be a `try`; the caller has already matched the head.
#[must_use]
pub fn examine_try(form: &ExpressionView) -> Vec<DeadClause> {
    let clauses = except_clauses(form);
    // An optimization, not a guard: a `try` with fewer than two handlers has no
    // earlier clause that could shadow a later one, so the loop below would
    // return empty anyway. Deleting this changes no result and kills no test.
    //
    // It is here because it is the shape almost all real code has. Over the
    // 3689-file third-party audit corpus, 1370 `try` forms carried a handler
    // and only **73** had two or more clauses — so roughly 95% of real
    // invocations stop here, before the `Caught` vector is even allocated.
    //
    // It deliberately does *not* speed up this package's benchmark, whose every
    // `try` carries a three-clause chain on purpose: that corpus measures the
    // comparison path, which is the one worth bounding. The README reports both
    // that figure and this distribution, because quoting the benchmark alone
    // would overstate what a lint run actually pays on Hy.
    if clauses.len() < 2 {
        return Vec::new();
    }
    let caught: Vec<Caught<'_>> = clauses.iter().map(|clause| caught_by(clause)).collect();
    let mut dead = Vec::new();

    // An unreadable clause is not skipped explicitly here: `covers` answers
    // `false` whenever either side is `Caught::Unknown`, so the search below
    // already declines it. An explicit `continue` was written first and deleted
    // after mutation testing showed it killed no test — it was a second copy of
    // a guard that lives in `covers`, and the duplication is exactly how the
    // sibling packages' two-walk bugs got in.
    for (index, later) in caught.iter().enumerate() {
        let Some(earlier_index) = (0..index).find(|&before| covers(&caught[before], later)) else {
            continue;
        };
        dead.push(DeadClause {
            span: clauses[index].span,
            position: index + 1,
            shadowed_by: earlier_index + 1,
            reason: shadow_reason(&caught[earlier_index], later),
        });
    }
    dead
}

fn shadow_reason(earlier: &Caught<'_>, later: &Caught<'_>) -> Shadow {
    let Caught::Types(before) = earlier else {
        return Shadow::CatchAll;
    };
    let Caught::Types(after) = later else {
        return Shadow::CatchAll;
    };
    // The first earlier type that covers the later clause's first type is the
    // one to name; a message that named all of them would be unreadable.
    let Some(first) = after.first() else {
        return Shadow::CatchAll;
    };
    for seen in before {
        if canonical(seen) == canonical(first) {
            return Shadow::SameType((*seen).to_owned());
        }
        if is_subtype_of(first, seen) {
            return Shadow::Supertype((*seen).to_owned());
        }
    }
    Shadow::CatchAll
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn dead(source: &str) -> Vec<DeadClause> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Hy).expect("parse");
        let root = tree.root_view();
        let form = root
            .children
            .iter()
            .find(|child| hy_head(child) == Some("try"))
            .expect("a try form");
        examine_try(form)
    }

    fn positions(source: &str) -> Vec<usize> {
        dead(source).into_iter().map(|item| item.position).collect()
    }

    /// Asserts what a single `(except …)` clause catches.
    ///
    /// Written as an assertion rather than as a function returning `Caught`,
    /// because `Caught` borrows the type names out of the tree: a helper that
    /// returned one would have to outlive the `root_view` it came from.
    fn assert_caught(source: &str, expected: &Caught<'_>) {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Hy).expect("parse");
        let root = tree.root_view();
        assert_eq!(&caught_by(&root.children[0]), expected, "{source}");
    }

    // -- the hierarchy ----------------------------------------------------

    #[test]
    fn a_type_is_a_subtype_of_itself() {
        assert!(is_subtype_of("ValueError", "ValueError"));
    }

    #[test]
    fn a_direct_child_is_a_subtype() {
        assert!(is_subtype_of("ValueError", "Exception"));
        assert!(is_subtype_of("KeyError", "LookupError"));
    }

    #[test]
    fn transitivity_is_computed_not_listed() {
        // UnicodeDecodeError -> UnicodeError -> ValueError -> Exception.
        assert!(is_subtype_of("UnicodeDecodeError", "ValueError"));
        assert!(is_subtype_of("UnicodeDecodeError", "Exception"));
        assert!(is_subtype_of("BrokenPipeError", "OSError"));
        assert!(is_subtype_of("TabError", "SyntaxError"));
    }

    #[test]
    fn the_relation_is_not_symmetric() {
        assert!(!is_subtype_of("Exception", "ValueError"));
        assert!(!is_subtype_of("LookupError", "KeyError"));
    }

    #[test]
    fn unrelated_builtins_are_unrelated() {
        assert!(!is_subtype_of("KeyError", "ValueError"));
        assert!(!is_subtype_of("OSError", "ArithmeticError"));
    }

    /// The one place an unknown class is answered for: everything catchable in
    /// Python derives from `BaseException`.
    #[test]
    fn base_exception_covers_even_an_unknown_class() {
        assert!(is_subtype_of("MyProjectError", "BaseException"));
        assert!(is_subtype_of("KeyboardInterrupt", "BaseException"));
    }

    /// The deliberate under-report. A project class may derive from
    /// `BaseException` directly, so `Exception` does not provably cover it.
    #[test]
    fn exception_does_not_cover_an_unknown_class() {
        assert!(!is_subtype_of("MyProjectError", "Exception"));
    }

    #[test]
    fn an_unknown_class_relates_to_nothing_but_itself() {
        assert!(is_subtype_of("MyProjectError", "MyProjectError"));
        assert!(!is_subtype_of("MyProjectError", "OtherError"));
    }

    /// `IOError` is not a subclass of `OSError`, it *is* `OSError`. The
    /// canonicalization has to work in both directions, which a subtype edge
    /// alone would not have given.
    #[test]
    fn the_builtin_aliases_are_the_same_class_both_ways() {
        assert!(is_subtype_of("IOError", "OSError"));
        assert!(is_subtype_of("OSError", "IOError"));
        assert!(is_subtype_of("EnvironmentError", "IOError"));
        assert!(is_subtype_of("FileNotFoundError", "IOError"));
    }

    // -- reading one clause -----------------------------------------------

    #[test]
    fn an_empty_binding_list_catches_everything() {
        assert_caught("(except [] 1)", &Caught::Everything);
    }

    #[test]
    fn a_named_base_exception_catches_everything_too() {
        assert_caught("(except [e BaseException] 1)", &Caught::Everything);
        assert_caught("(except [BaseException] 1)", &Caught::Everything);
    }

    #[test]
    fn a_lone_type_has_no_bound_name() {
        assert_caught(
            "(except [ValueError] 1)",
            &Caught::Types(vec!["ValueError"]),
        );
    }

    #[test]
    fn a_bound_name_and_type_reads_the_second_element() {
        assert_caught(
            "(except [e ValueError] 1)",
            &Caught::Types(vec!["ValueError"]),
        );
    }

    #[test]
    fn a_tuple_of_types_reads_every_element() {
        assert_caught(
            "(except [e [ValueError KeyError]] 1)",
            &Caught::Types(vec!["ValueError", "KeyError"]),
        );
    }

    /// A one-element binding list holding a *list* is a tuple of types with no
    /// bound name, so arity alone cannot decide which element is the type.
    #[test]
    fn a_tuple_of_types_without_a_bound_name_is_read_as_types() {
        assert_caught(
            "(except [[ValueError KeyError]] 1)",
            &Caught::Types(vec!["ValueError", "KeyError"]),
        );
    }

    #[test]
    fn a_computed_type_is_unknown() {
        assert_caught("(except [e (pick-error)] 1)", &Caught::Unknown);
        assert_caught("(except [e [ValueError (f)]] 1)", &Caught::Unknown);
    }

    #[test]
    fn a_binding_list_that_is_not_a_bracket_list_is_unknown() {
        assert_caught("(except (e ValueError) 1)", &Caught::Unknown);
    }

    #[test]
    fn a_clause_with_no_binding_list_at_all_is_unknown() {
        assert_caught("(except)", &Caught::Unknown);
    }

    // -- whole forms ------------------------------------------------------

    #[test]
    fn a_correctly_ordered_try_is_clean() {
        assert_eq!(
            positions("(try (f) (except [e ValueError] 1) (except [e Exception] 2))"),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn a_supertype_first_kills_the_narrow_clause() {
        assert_eq!(
            positions("(try (f) (except [e Exception] 1) (except [e ValueError] 2))"),
            vec![2]
        );
    }

    #[test]
    fn a_duplicate_type_kills_the_second_clause() {
        assert_eq!(
            positions("(try (f) (except [e ValueError] 1) (except [e ValueError] 2))"),
            vec![2]
        );
    }

    #[test]
    fn a_bare_clause_kills_every_later_clause() {
        assert_eq!(
            positions("(try (f) (except [] 1) (except [e ValueError] 2) (except [e KeyError] 3))"),
            vec![2, 3]
        );
    }

    #[test]
    fn a_multi_type_clause_is_dead_only_when_every_type_is_covered() {
        // `OSError` is not covered by `LookupError`, so the clause still runs.
        assert_eq!(
            positions("(try (f) (except [e LookupError] 1) (except [e [KeyError OSError]] 2))"),
            Vec::<usize>::new()
        );
        assert_eq!(
            positions("(try (f) (except [e LookupError] 1) (except [e [KeyError IndexError]] 2))"),
            vec![2]
        );
    }

    #[test]
    fn an_unknown_clause_neither_dies_nor_kills() {
        assert_eq!(
            positions("(try (f) (except [e (pick)] 1) (except [e ValueError] 2))"),
            Vec::<usize>::new()
        );
        assert_eq!(
            positions("(try (f) (except [e Exception] 1) (except [e (pick)] 2))"),
            Vec::<usize>::new()
        );
    }

    /// `else` and `finally` are siblings of the except clauses; counting them
    /// as clauses would shift every reported position.
    #[test]
    fn else_and_finally_are_not_except_clauses() {
        assert_eq!(
            positions(
                "(try (f) (except [e ValueError] 1) (except [e ValueError] 2) (else 3) (finally 4))"
            ),
            vec![2]
        );
    }

    /// The body sits before the clauses, and the position counts clauses only.
    #[test]
    fn the_position_is_counted_among_except_clauses_only() {
        let found = dead("(try (setup) (work) (except [e Exception] 1) (except [e KeyError] 2))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].position, 2);
        assert_eq!(found[0].shadowed_by, 1);
    }

    #[test]
    fn a_try_with_no_clauses_reports_nothing() {
        assert_eq!(positions("(try (f))"), Vec::<usize>::new());
        assert_eq!(positions("(try (f) (finally 1))"), Vec::<usize>::new());
    }

    #[test]
    fn the_reason_names_the_shadowing_clause() {
        let same = dead("(try (f) (except [e ValueError] 1) (except [e ValueError] 2))");
        assert_eq!(same[0].reason, Shadow::SameType("ValueError".to_owned()));

        let broader = dead("(try (f) (except [e Exception] 1) (except [e ValueError] 2))");
        assert_eq!(broader[0].reason, Shadow::Supertype("Exception".to_owned()));

        let all = dead("(try (f) (except [] 1) (except [e ValueError] 2))");
        assert_eq!(all[0].reason, Shadow::CatchAll);
    }

    /// Every dead clause is reported, not only the first.
    #[test]
    fn more_than_one_dead_clause_is_reported() {
        assert_eq!(
            positions(
                "(try (f) (except [e Exception] 1) (except [e ValueError] 2) (except [e KeyError] 3))"
            ),
            vec![2, 3]
        );
    }

    /// The finding points at the dead clause, not at the whole `try` — a rule
    /// reporting the enclosing form would be useless on a long handler chain.
    #[test]
    fn the_span_is_the_clause_not_the_try() {
        let source = "(try (f) (except [e Exception] 1) (except [e KeyError] 2))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Hy).expect("parse");
        let try_span = tree.root_view().children[0].span;
        let found = dead(source);
        assert_ne!(found[0].span, try_span);
        assert_eq!(
            &source[found[0].span.start().get()..found[0].span.end().get()],
            "(except [e KeyError] 2)"
        );
    }
}
