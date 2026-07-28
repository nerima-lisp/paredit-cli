//! Small shared queries and traversals over parsed [`ExpressionView`]s.
//!
//! Every report that walks a whole form tree looking for a particular head
//! (`case`, `cond`, `if`, `setf`, …) needs the same handful of one-liners —
//! "is this a parenthesized list", "what is its head symbol", "the atom text
//! of child N" — plus the same pre-order recursion. Keeping one copy here
//! means a report body is just its own analysis, not re-derived boilerplate.

use crate::sexpr::{Delimiter, ExpressionKind, ExpressionView};

/// The atom's text, or `None` for a non-atom (or a text-less atom).
#[must_use]
pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The atom text of the child at `index`, if that child exists and is an atom.
pub fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

/// Whether `view` is a `(...)` list (as opposed to an atom, a `[...]`
/// bracket, or a `{...}` brace form).
#[must_use]
pub fn is_paren_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren)
}

/// The head symbol of a `(...)` list — its first child's atom text — or
/// `None` if `view` is not a paren list or its head is not a bare symbol.
#[must_use]
pub fn list_head(view: &ExpressionView) -> Option<&str> {
    is_paren_list(view).then(|| atom_child(view, 0)).flatten()
}

/// The symbol without its package qualifier: `cl:append` → `append`.
///
/// Splits on the *last* marker so `pkg:name` and `pkg::internal` both reduce to
/// the name, and leaves a bare `:keyword` alone — a keyword's name has no
/// package part, and stripping its colon would make `:test` compare equal to a
/// function named `test`.
#[must_use]
pub fn unqualified(symbol: &str) -> &str {
    if symbol.starts_with(':') {
        return symbol;
    }
    match symbol.rsplit_once(':') {
        Some((_, name)) if !name.is_empty() => name,
        _ => symbol,
    }
}

/// Whether `symbol` names `expected`, ignoring case and any package qualifier.
///
/// The reader upcases unescaped symbols and a package prefix does not change
/// which function is named, so `CL:APPEND`, `cl:append`, and `append` are one
/// operator. Comparing raw text would silently skip the qualified spelling,
/// which is the spelling a large codebase uses.
#[must_use]
pub fn symbol_is(symbol: &str, expected: &str) -> bool {
    unqualified(symbol).eq_ignore_ascii_case(expected)
}

/// Whether `symbol` names any of `expected`.
#[must_use]
pub fn symbol_in(symbol: &str, expected: &[&str]) -> bool {
    expected.iter().any(|name| symbol_is(symbol, name))
}

/// Whether `view` is a `(...)` call to one of `heads`.
#[must_use]
pub fn calls(view: &ExpressionView, heads: &[&str]) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, heads))
}

/// Calls `visit` on `root` and every descendant view, in pre-order (a view
/// before its children). The single place the whole-tree recursion that
/// body-form lints share is written.
pub fn for_each_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    fn recurse(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
        visit(view);
        for child in &view.children {
            recurse(child, visit);
        }
    }
    recurse(root, &mut visit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr::{Path, SyntaxTree};

    fn root(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse(input).expect("parse input");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    #[test]
    fn list_head_reads_the_first_symbol() {
        assert_eq!(list_head(&root("(defun f ())")), Some("defun"));
    }

    #[test]
    fn list_head_is_none_for_an_atom() {
        assert_eq!(list_head(&root("x")), None);
    }

    #[test]
    fn atom_child_reads_a_positional_symbol() {
        assert_eq!(atom_child(&root("(defun f ())"), 1), Some("f"));
    }

    #[test]
    fn symbol_comparison_ignores_case_and_package() {
        assert!(symbol_is("APPEND", "append"));
        assert!(symbol_is("cl:append", "append"));
        assert!(symbol_is("COMMON-LISP::APPEND", "append"));
        assert!(!symbol_is("appendf", "append"));
    }

    #[test]
    fn a_keyword_keeps_its_colon_so_it_is_not_a_function_name() {
        assert_eq!(unqualified(":test"), ":test");
        assert!(!symbol_is(":test", "test"));
    }

    #[test]
    fn calls_reads_a_qualified_head() {
        assert!(calls(&root("(cl:append a b)"), &["append"]));
        assert!(!calls(&root("(nconc a b)"), &["append"]));
        assert!(!calls(&root("x"), &["append"]));
    }

    #[test]
    fn for_each_subview_visits_root_and_all_descendants_preorder() {
        let mut heads = Vec::new();
        for_each_subview(&root("(a (b) (c (d)))"), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        assert_eq!(heads, vec!["a", "b", "c", "d"]);
    }
}
