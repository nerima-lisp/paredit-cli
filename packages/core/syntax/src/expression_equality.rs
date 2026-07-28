//! Structural comparison and display of parsed expressions, shared by the
//! reports that need to ask "are these two subtrees the same code?" —
//! `self_assignment_report` (`(setq x x)`) and
//! `identical_if_branch_report` (`(if c a a)`).
//!
//! Equality follows Common Lisp reader semantics: two symbols are equal when
//! their spellings match after folding ASCII case (the reader upcases
//! unescaped symbol characters), while string (`"..."`) and character
//! (`#\...`) literals stay case-sensitive because distinct strings are not
//! `eql`. A package-qualified symbol is therefore distinct from an
//! unqualified one, since their spellings differ. Reader prefixes (`'`, `` ` ``,
//! `,`) are not compared here — the callers work over places and branch
//! bodies where those do not appear at the top level.

use crate::sexpr::{Delimiter, ExpressionKind, ExpressionView};

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// Compares two atoms the way `eql` would see the read objects: symbol
/// spellings fold ASCII case, while string and character literals stay
/// case-sensitive.
fn atoms_equal(left: &str, right: &str) -> bool {
    let is_literal = |text: &str| text.starts_with('"') || text.starts_with("#\\");
    if is_literal(left) || is_literal(right) {
        left == right
    } else {
        left.eq_ignore_ascii_case(right)
    }
}

/// Structural equality of two expression views: same kind and delimiter, and
/// recursively equal children, with atoms compared by reader-aware
/// `atoms_equal`.
#[must_use]
pub fn expressions_structurally_equal(left: &ExpressionView, right: &ExpressionView) -> bool {
    if left.kind != right.kind || left.delimiter != right.delimiter {
        return false;
    }
    match left.kind {
        ExpressionKind::Atom => match (atom_text(left), atom_text(right)) {
            (Some(left_text), Some(right_text)) => atoms_equal(left_text, right_text),
            _ => false,
        },
        _ => {
            left.children.len() == right.children.len()
                && left
                    .children
                    .iter()
                    .zip(&right.children)
                    .all(|(left_child, right_child)| {
                        expressions_structurally_equal(left_child, right_child)
                    })
        }
    }
}

/// Renders a view back to an approximate S-expression string for display in
/// a report (not a round-trippable serialization — reader prefixes and exact
/// whitespace are not preserved).
pub fn render_expression(view: &ExpressionView) -> String {
    match view.kind {
        ExpressionKind::Atom => view.text.clone().unwrap_or_default(),
        _ => {
            let (open, close) = match view.delimiter {
                Some(Delimiter::Paren) | None => ("(", ")"),
                Some(Delimiter::Bracket) => ("[", "]"),
                Some(Delimiter::Brace) => ("{", "}"),
            };
            let inner = view
                .children
                .iter()
                .map(render_expression)
                .collect::<Vec<_>>()
                .join(" ");
            format!("{open}{inner}{close}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr::SyntaxTree;

    fn first_two_children(input: &str) -> (ExpressionView, ExpressionView) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        let root = tree
            .select_path(&crate::sexpr::Path::root_child(0))
            .expect("root form")
            .view();
        (root.children[1].clone(), root.children[2].clone())
    }

    #[test]
    fn equal_symbols_fold_case() {
        let (left, right) = first_two_children("(pair Foo foo)");
        assert!(expressions_structurally_equal(&left, &right));
    }

    #[test]
    fn distinct_symbols_are_unequal() {
        let (left, right) = first_two_children("(pair foo bar)");
        assert!(!expressions_structurally_equal(&left, &right));
    }

    #[test]
    fn string_literals_stay_case_sensitive() {
        let (left, right) = first_two_children("(pair \"x\" \"X\")");
        assert!(!expressions_structurally_equal(&left, &right));
    }

    #[test]
    fn nested_lists_compare_recursively() {
        let (left, right) = first_two_children("(pair (aref a i) (aref A i))");
        assert!(expressions_structurally_equal(&left, &right));
    }

    #[test]
    fn different_arity_lists_are_unequal() {
        let (left, right) = first_two_children("(pair (f a) (f a b))");
        assert!(!expressions_structurally_equal(&left, &right));
    }

    #[test]
    fn render_reconstructs_a_nested_form() {
        let (left, _) = first_two_children("(pair (aref a i) x)");
        assert_eq!(render_expression(&left), "(aref a i)");
    }
}
