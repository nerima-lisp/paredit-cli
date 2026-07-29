//! Matching a [`Pattern`] against every form in a document.
//!
//! Two properties the callers depend on:
//!
//! - **Results are in source order**, because a `--all` edit applies them in
//!   reverse and a report prints them top to bottom.
//! - **Matching is pre-order and does not stop at a match**, so
//!   `--query '(let ...)'` finds a nested `let` inside an outer one. Selecting
//!   only outermost matches would silently hide the inner form, and a caller
//!   that wants only the outer one can say so with a more specific pattern.

use crate::dialect::Dialect;
use crate::sexpr::{ByteSpan, ExpressionKind, ExpressionPath, ExpressionView, SyntaxTree};

use super::normalize::normalized_form_text;
use super::pattern::{Pattern, Rest};

/// What one capture name bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub name: String,
    /// The bound forms. One entry for `?name`, zero or more for `?name...`.
    pub paths: Vec<ExpressionPath>,
    /// The span covering every bound form, or `None` when a rest bound none.
    pub span: Option<ByteSpan>,
    /// The bound source text, whitespace-normalized and space-joined.
    pub text: String,
}

/// One form that satisfied the pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternMatch {
    pub path: ExpressionPath,
    pub span: ByteSpan,
    /// Captures in name order, so output is stable across runs.
    pub captures: Vec<Capture>,
}

impl PatternMatch {
    /// The capture bound to `name`, if the pattern bound one.
    #[must_use]
    pub fn capture(&self, name: &str) -> Option<&Capture> {
        self.captures.iter().find(|capture| capture.name == name)
    }
}

/// Every form in `tree` that `pattern` matches, in source order.
#[must_use]
pub fn match_all(tree: &SyntaxTree, pattern: &Pattern, dialect: Dialect) -> Vec<PatternMatch> {
    let root = tree.root_view();
    let source = tree.source();
    let mut matches = Vec::new();

    // An explicit worklist rather than recursion: document depth is whatever
    // the input file has, and this walk is the one that meets untrusted trees.
    let mut pending: Vec<(&ExpressionView, ExpressionPath)> = root
        .children
        .iter()
        .enumerate()
        .rev()
        .map(|(index, child)| (child, ExpressionPath::root_child(index)))
        .collect();

    while let Some((view, path)) = pending.pop() {
        let mut bindings = Vec::new();
        if matches_here(pattern, view, &path, source, dialect, &mut bindings) {
            matches.push(PatternMatch {
                path: path.clone(),
                span: view.span,
                captures: into_captures(bindings, source),
            });
        }
        pending.extend(
            view.children
                .iter()
                .enumerate()
                .rev()
                .map(|(index, child)| (child, path.child(index))),
        );
    }

    matches.sort_by_key(|found| (found.span.start(), found.span.end()));
    matches
}

/// A capture binding while matching is still in progress.
struct Binding<'a> {
    name: String,
    views: Vec<(&'a ExpressionView, ExpressionPath)>,
}

fn into_captures(bindings: Vec<Binding<'_>>, source: &str) -> Vec<Capture> {
    let mut captures: Vec<Capture> = bindings
        .into_iter()
        .map(|binding| {
            let span = binding.views.first().zip(binding.views.last()).and_then(
                |((first, _), (last, _))| ByteSpan::try_new(first.span.start(), last.span.end()),
            );
            Capture {
                name: binding.name,
                paths: binding.views.iter().map(|(_, path)| path.clone()).collect(),
                span,
                text: binding
                    .views
                    .iter()
                    .map(|(view, _)| normalized_form_text(source, view.span))
                    .collect::<Vec<_>>()
                    .join(" "),
            }
        })
        .collect();
    captures.sort_by(|left, right| left.name.cmp(&right.name));
    captures
}

fn matches_here<'a>(
    pattern: &Pattern,
    view: &'a ExpressionView,
    path: &ExpressionPath,
    source: &str,
    dialect: Dialect,
    bindings: &mut Vec<Binding<'a>>,
) -> bool {
    match pattern {
        Pattern::Wildcard {
            capture,
            kind,
            prefixes,
        } => {
            // An unprefixed wildcard is prefix-agnostic; see `Pattern::Wildcard`.
            if !prefixes.is_empty() && view.reader_prefixes != *prefixes {
                return false;
            }
            if !kind.accepts(view) {
                return false;
            }
            match capture {
                None => true,
                Some(name) => bind(bindings, name, vec![(view, path.clone())], source),
            }
        }
        Pattern::Atom { text, prefixes } => {
            view.kind == ExpressionKind::Atom
                && view.reader_prefixes == *prefixes
                && symbol_matches(atom_symbol(view), text, dialect)
        }
        Pattern::List {
            delimiter,
            prefixes,
            before,
            rest,
            after,
        } => {
            if view.kind != ExpressionKind::List
                || view.delimiter != Some(*delimiter)
                || view.reader_prefixes != *prefixes
            {
                return false;
            }
            match_children(before, rest, after, view, path, source, dialect, bindings)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn match_children<'a>(
    before: &[Pattern],
    rest: &Option<Rest>,
    after: &[Pattern],
    view: &'a ExpressionView,
    path: &ExpressionPath,
    source: &str,
    dialect: Dialect,
    bindings: &mut Vec<Binding<'a>>,
) -> bool {
    let children = view.children.as_slice();
    let Some(rest) = rest else {
        return children.len() == before.len()
            && before
                .iter()
                .zip(children)
                .enumerate()
                .all(|(index, (pattern, child))| {
                    matches_here(
                        pattern,
                        child,
                        &path.child(index),
                        source,
                        dialect,
                        bindings,
                    )
                });
    };

    // With a rest, the fixed patterns anchor to both ends and the rest takes
    // whatever is left. That is unambiguous precisely because a list pattern
    // is limited to one rest, which is why `parse` enforces the limit.
    let fixed = before.len() + after.len();
    if children.len() < fixed {
        return false;
    }

    for (index, pattern) in before.iter().enumerate() {
        if !matches_here(
            pattern,
            &children[index],
            &path.child(index),
            source,
            dialect,
            bindings,
        ) {
            return false;
        }
    }

    let tail_start = children.len() - after.len();
    for (offset, pattern) in after.iter().enumerate() {
        let index = tail_start + offset;
        if !matches_here(
            pattern,
            &children[index],
            &path.child(index),
            source,
            dialect,
            bindings,
        ) {
            return false;
        }
    }

    match &rest.capture {
        None => true,
        Some(name) => {
            let swallowed = (before.len()..tail_start)
                .map(|index| (&children[index], path.child(index)))
                .collect();
            bind(bindings, name, swallowed, source)
        }
    }
}

/// Binds `name`, or checks the back-reference when it is already bound.
///
/// A repeated name is what makes `(eq ?x ?x)` mean "compared with itself"
/// rather than "compared with anything". Equality is on normalized text, so
/// `(eq  x   x)` still matches while `(eq x y)` does not.
fn bind<'a>(
    bindings: &mut Vec<Binding<'a>>,
    name: &str,
    views: Vec<(&'a ExpressionView, ExpressionPath)>,
    source: &str,
) -> bool {
    let rendered = |views: &[(&ExpressionView, ExpressionPath)]| {
        views
            .iter()
            .map(|(view, _)| normalized_form_text(source, view.span))
            .collect::<Vec<_>>()
            .join(" ")
    };

    match bindings.iter().find(|binding| binding.name == name) {
        Some(existing) => rendered(&existing.views) == rendered(&views),
        None => {
            bindings.push(Binding {
                name: name.to_owned(),
                views,
            });
            true
        }
    }
}

fn atom_symbol(view: &ExpressionView) -> &str {
    let text = view.text.as_deref().unwrap_or_default();
    &text[view.symbol_offset.min(text.len())..]
}

/// Compares a literal pattern atom with a source atom.
///
/// Only Common Lisp folds case and package qualifiers, because only there does
/// the reader do it: `DEFUN`, `defun` and `cl:defun` name one operator. Emacs
/// Lisp, Scheme, Racket and Clojure are case-sensitive, and folding case for
/// them would make `--query '(Foo)'` match `(foo)` in a language where those
/// are two different symbols.
pub(super) fn symbol_matches(candidate: &str, expected: &str, dialect: Dialect) -> bool {
    if dialect == Dialect::CommonLisp {
        crate::common_lisp::common_lisp_symbol_reference_eq(candidate, expected)
    } else {
        candidate == expected
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn found(source: &str, pattern: &str) -> Vec<PatternMatch> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).unwrap();
        let pattern = Pattern::parse(pattern, Dialect::CommonLisp).unwrap();
        match_all(&tree, &pattern, Dialect::CommonLisp)
    }

    #[test]
    fn a_definition_pattern_finds_every_definition_in_source_order() {
        let matches = found(
            "(defun a ()) (defvar *b*) (defun c (x) x)",
            "(defun ?name ...)",
        );
        let names = matches
            .iter()
            .map(|found| found.capture("name").unwrap().text.clone())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a".to_owned(), "c".to_owned()]);
        assert_eq!(
            matches
                .iter()
                .map(|found| found.path.to_string())
                .collect::<Vec<_>>(),
            vec!["0".to_owned(), "2".to_owned()]
        );
    }

    #[test]
    fn arity_is_exact_without_a_rest() {
        assert_eq!(found("(if a b) (if a b c)", "(if ?test ?then)").len(), 1);
    }

    #[test]
    fn a_repeated_capture_is_a_back_reference() {
        let matches = found("(eq x x) (eq x y) (eq  x   x )", "(eq ?x ?x)");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn nested_matches_are_reported_as_well_as_the_outer_one() {
        let matches = found("(let ((a 1)) (let ((b 2)) b))", "(let ...)");
        assert_eq!(matches.len(), 2);
        assert_eq!(
            matches
                .iter()
                .map(|found| found.path.to_string())
                .collect::<Vec<_>>(),
            vec!["0".to_owned(), "0.2".to_owned()]
        );
    }

    #[test]
    fn a_rest_capture_binds_the_run_it_swallowed() {
        let matches = found("(progn a b c)", "(progn ?body...)");
        let body = matches[0].capture("body").unwrap();
        assert_eq!(body.text, "a b c");
        assert_eq!(body.paths.len(), 3);
    }

    #[test]
    fn an_empty_rest_binds_nothing_and_still_matches() {
        let matches = found("(progn)", "(progn ?body...)");
        let body = matches[0].capture("body").unwrap();
        assert!(body.paths.is_empty());
        assert_eq!(body.span, None);
    }

    #[test]
    fn a_trailing_pattern_anchors_to_the_end_of_the_list() {
        let matches = found("(defun f (x) a b result)", "(defun ?name ... ?last)");
        assert_eq!(matches[0].capture("last").unwrap().text, "result");
    }

    #[test]
    fn a_kind_constrains_what_a_capture_accepts() {
        assert_eq!(found("(f 1) (f x) (f (g))", "(f ?a:number)").len(), 1);
        assert_eq!(found("(f 1) (f x) (f (g))", "(f ?a:list)").len(), 1);
        assert_eq!(found("(f 1) (f x) (f (g))", "(f ?a:symbol)").len(), 1);
    }

    #[test]
    fn common_lisp_folds_case_and_package_qualifiers() {
        assert_eq!(
            found("(DEFUN a ()) (cl:defun b ())", "(defun ?n ...)").len(),
            2
        );
    }

    #[test]
    fn a_case_sensitive_dialect_does_not_fold() {
        let tree = SyntaxTree::parse_with_dialect("(Defun a)", Dialect::Clojure).unwrap();
        let pattern = Pattern::parse("(defun ?n)", Dialect::Clojure).unwrap();
        assert!(match_all(&tree, &pattern, Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_delimiter_is_part_of_the_pattern() {
        let tree = SyntaxTree::parse_with_dialect("(let [a 1] a)", Dialect::Clojure).unwrap();
        let brackets = Pattern::parse("[?name ...]", Dialect::Clojure).unwrap();
        let parens = Pattern::parse("(?name ...)", Dialect::Clojure).unwrap();
        assert_eq!(match_all(&tree, &brackets, Dialect::Clojure).len(), 1);
        assert_eq!(match_all(&tree, &parens, Dialect::Clojure).len(), 1);
    }

    #[test]
    fn a_reader_prefix_in_the_pattern_must_be_present_in_the_source() {
        assert_eq!(
            found("(mapcar #'f xs) (mapcar f xs)", "(mapcar #'?fn ?xs)").len(),
            1
        );
    }
}
