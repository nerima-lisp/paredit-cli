//! Shared, deliberately conservative queries for the `do`/`dolist`/`dotimes`
//! rules in this package.
//!
//! Everything here is biased the same way: when a question cannot be answered
//! from the sub-tree alone, the answer that *suppresses* a finding is the one
//! returned. The three rules built on this module all report on user code that
//! looks correct, so a false positive costs far more than a false negative.
//!
//! Scope: Common Lisp only.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ExpressionView, ReaderPrefix};
use paredit_core_syntax::view_query::{is_paren_list, list_head, unqualified};

/// Reader prefixes that make the form beneath them unevaluated data — a
/// literal, or a macro template whose contents are not this call's code.
///
/// `Quasiquote` is included even though an unquote can re-introduce
/// evaluation: descending into a template and reporting on it is how a lint
/// rule warns about the *expansion* it never saw.
fn is_unevaluated(view: &ExpressionView) -> bool {
    view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::Quote
                | ReaderPrefix::Quasiquote
                | ReaderPrefix::Function
                | ReaderPrefix::ReadEval
                | ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
        )
    })
}

/// The symbol a view names, if it reads as a plain variable name.
///
/// Rejects anything that cannot be a variable a body assigns or reads: a
/// keyword, a string, a number, a reader-macro atom (`#+sbcl`), and the two
/// constants `t` and `nil`. The text is returned lowercased and
/// package-unqualified, which is how every comparison in this package is made.
#[must_use]
pub fn plain_variable_name(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_symbol_text(view)?;
    if text.is_empty() || text.starts_with(':') || text.starts_with('"') || text.starts_with('#') {
        return None;
    }
    if text.starts_with(|character: char| character.is_ascii_digit()) {
        return None;
    }
    let name = unqualified(text).to_ascii_lowercase();
    if name.is_empty() || name == "t" || name == "nil" {
        return None;
    }
    Some(name)
}

/// Whether `view` is an atom naming `name`, ignoring case and package
/// qualifier.
///
/// A string literal keeps its quotes in `text`, so `"total"` never names
/// `total`.
#[must_use]
pub fn names_variable(view: &ExpressionView, name: &str) -> bool {
    if !view.reader_prefixes.is_empty() {
        return false;
    }
    atom_names(view, name)
}

/// Whether an atom names `name`, ignoring case and package qualifier.
fn atom_names(view: &ExpressionView, name: &str) -> bool {
    atom_symbol_text(view).is_some_and(|text| unqualified(text).eq_ignore_ascii_case(name))
}

/// Calls `visit` on `root` and every descendant that is actually evaluated,
/// pre-order, never descending beneath a quote or quasiquote.
///
/// This is the walk every rule in this package uses instead of
/// [`paredit_core_syntax::view_query::for_each_subview`], whose whole-tree
/// recursion cannot tell a `(setq n 0)` that runs from one inside `'(setq n
/// 0)` that does not.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    fn recurse(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
        if is_unevaluated(view) {
            return;
        }
        visit(view);
        for child in &view.children {
            recurse(child, visit);
        }
    }
    recurse(root, &mut visit);
}

/// Binding operators whose binding specification is their second element.
const SPEC_AT_ONE: [&str; 18] = [
    "let",
    "let*",
    "symbol-macrolet",
    "macrolet",
    "flet",
    "labels",
    "prog",
    "prog*",
    "do",
    "do*",
    "dolist",
    "dotimes",
    "lambda",
    "multiple-value-bind",
    "destructuring-bind",
    "with-slots",
    "with-accessors",
    "with-open-file",
];

/// Definition operators whose lambda list is their third element.
const SPEC_AT_TWO: [&str; 4] = ["defun", "defmacro", "defmethod", "define-compiler-macro"];

/// Operators whose bindings this module does not locate precisely, and which
/// therefore count as binding *any* name they mention anywhere.
///
/// `loop` is the important one: it binds through a grammar rather than a
/// binding list, so "does this `loop` bind `n`?" cannot be answered from a
/// child index. Treating a mention as a binding is the suppressing answer.
const OPAQUE_BINDERS: [&str; 8] = [
    "loop",
    "handler-case",
    "handler-bind",
    "restart-case",
    "restart-bind",
    "with-hash-table-iterator",
    "with-package-iterator",
    "with-input-from-string",
];

/// Whether any atom anywhere beneath `view` names `name`.
fn mentions(view: &ExpressionView, name: &str) -> bool {
    if atom_names(view, name) {
        return true;
    }
    view.children.iter().any(|child| mentions(child, name))
}

/// Whether anything beneath `root` might rebind `name`, shadowing the
/// outer binding a rule is reasoning about.
///
/// Answers "might", not "does". A `(let ((m n)) …)` merely *reads* `n` in an
/// init form, yet its binding list mentions `n` and so this returns `true`.
/// That costs a finding and buys the guarantee that a rule never reports on a
/// variable whose name it has confused with an inner one.
#[must_use]
pub fn may_rebind(root: &ExpressionView, name: &str) -> bool {
    let mut found = false;
    for_each_evaluated_subview(root, |view| {
        if found {
            return;
        }
        let Some(head) = list_head(view) else { return };
        let head = unqualified(head).to_ascii_lowercase();
        let spec = if SPEC_AT_ONE.contains(&head.as_str()) {
            view.children.get(1)
        } else if SPEC_AT_TWO.contains(&head.as_str()) {
            view.children.get(2)
        } else if OPAQUE_BINDERS.contains(&head.as_str()) {
            Some(view)
        } else {
            None
        };
        if spec.is_some_and(|spec| mentions(spec, name)) {
            found = true;
        }
    });
    found
}

/// Whether `name` is read as a variable anywhere beneath `root`.
///
/// An atom in operator position (`(count …)`) is a function name, not a
/// variable read, and does not count. Quoted sub-forms do not count either.
#[must_use]
pub fn references_variable(root: &ExpressionView, name: &str) -> bool {
    fn recurse(view: &ExpressionView, name: &str) -> bool {
        if is_unevaluated(view) {
            return false;
        }
        if atom_names(view, name) {
            return true;
        }
        // Skip child 0 of a `(...)` call: it is the operator being applied.
        let skip_operator = usize::from(is_paren_list(view) && list_head(view).is_some());
        view.children
            .iter()
            .skip(skip_operator)
            .any(|child| recurse(child, name))
    }
    recurse(root, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn with_form<T>(input: &str, check: impl FnOnce(&ExpressionView) -> T) -> T {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("root form")
            .view();
        check(&view)
    }

    #[test]
    fn a_plain_symbol_is_a_variable_name() {
        assert_eq!(
            with_form("count", plain_variable_name),
            Some("count".to_owned())
        );
        assert_eq!(
            with_form("cl-user::total", plain_variable_name),
            Some("total".to_owned())
        );
    }

    #[test]
    fn constants_keywords_literals_and_reader_macros_are_not_variable_names() {
        assert_eq!(with_form("nil", plain_variable_name), None);
        assert_eq!(with_form("t", plain_variable_name), None);
        assert_eq!(with_form(":key", plain_variable_name), None);
        assert_eq!(with_form("\"text\"", plain_variable_name), None);
        assert_eq!(with_form("42", plain_variable_name), None);
        assert_eq!(with_form("'x", plain_variable_name), None);
    }

    /// A reader conditional folds into a single atom under the dialect-aware
    /// parse, and that atom is not a variable name.
    #[test]
    fn a_reader_conditional_is_not_a_variable_name() {
        let name = with_form("(f #+sbcl x)", |view| {
            plain_variable_name(view.children.get(1).expect("second child"))
        });
        assert_eq!(name, None);
    }

    #[test]
    fn a_reference_in_operator_position_is_not_a_variable_read() {
        assert!(!with_form("(count items)", |view| references_variable(
            view, "count"
        )));
        assert!(with_form("(length count)", |view| references_variable(
            view, "count"
        )));
    }

    #[test]
    fn a_quoted_reference_is_not_a_variable_read() {
        assert!(!with_form("(list 'total)", |view| references_variable(
            view, "total"
        )));
        assert!(!with_form("(list `(total))", |view| references_variable(
            view, "total"
        )));
        assert!(with_form("(list total)", |view| references_variable(
            view, "total"
        )));
    }

    #[test]
    fn a_string_containing_the_name_is_not_a_variable_read() {
        assert!(!with_form("(print \"total\")", |view| references_variable(
            view, "total"
        )));
    }

    #[test]
    fn a_let_over_the_same_name_counts_as_a_possible_rebind() {
        assert!(with_form("(progn (let ((n 1)) n))", |view| may_rebind(
            view, "n"
        )));
        assert!(!with_form("(progn (let ((m 1)) m))", |view| may_rebind(
            view, "n"
        )));
    }

    #[test]
    fn a_loop_mentioning_the_name_counts_as_a_possible_rebind() {
        // `loop` binds through its own grammar, so any mention suppresses.
        assert!(with_form(
            "(progn (loop for n from 1 to 3 do (f n)))",
            |view| { may_rebind(view, "n") }
        ));
    }

    #[test]
    fn a_quoted_binding_form_is_not_a_rebind() {
        assert!(!with_form("(progn '(let ((n 1)) n))", |view| may_rebind(
            view, "n"
        )));
    }

    #[test]
    fn the_evaluated_walk_does_not_descend_into_quoted_data() {
        let mut heads = Vec::new();
        with_form("(progn (a) '(b) (c))", |view| {
            for_each_evaluated_subview(view, |subview| {
                if let Some(head) = list_head(subview) {
                    heads.push(head.to_owned());
                }
            });
        });
        assert_eq!(heads, vec!["progn", "a", "c"]);
    }
}
