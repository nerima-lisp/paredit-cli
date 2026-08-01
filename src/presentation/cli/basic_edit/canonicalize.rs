//! `edit canonicalize`'s transformation: sort an alist- or plist-shaped
//! list's entries by key, and flatten every list's own whitespace to a
//! single space between elements — without ever touching a file this cannot
//! confidently call data.
//!
//! This is deliberately not `edit format`. `edit format` renders *code*: an
//! indentation table per operator, threading-macro handling, a line-width
//! budget. A `paredit.toml`-adjacent data file — an alist of settings, a
//! plist of properties — is not read as nested code blocks, so none of that
//! belongs here. What it needs instead is "boring and consistent"
//! whitespace and a deterministic key order, and nothing else touched.
//!
//! # Why every atom is copied, never reprinted
//!
//! The single rule that makes this safe: an atom's rendered text is always
//! `source[view.span]`, a verbatim substring, never something reconstructed
//! from a parsed value. A float prints back exactly because it was never
//! parsed as a float. A string's escapes, embedded newlines, and quotes
//! round-trip byte for byte because they were never re-escaped. This also
//! means a reader-prefixed node — `#+feature (form)`, `'quoted-list`,
//! `` `(quasiquoted) `` — can be copied whole (`view.span` includes the
//! prefix) and left completely alone: it is never descended into, so a
//! quoted list's internal order, which is the author's choice and not
//! necessarily alist/plist data to re-sort, is never disturbed.
//!
//! # Why shape detection is duplicated rather than imported
//!
//! `paredit-feature-data-report`'s `data_check_report::domain` has the same
//! "is this a plist/alist" logic, but keeps it private — it is tuned for
//! *reporting* a duplicate key or a dangling plist entry, not for deciding
//! whether to *reorder* one, and the two callers' conservatism needs differ
//! enough (this one also refuses to reorder a reader-prefixed key or entry,
//! which reporting has no reason to care about) that widening a report
//! crate's private API to serve a write command in a different command
//! family was the wrong direction to reach for. The shape test itself is
//! small and built from the same public `paredit_core_syntax::view_query`
//! primitives that crate already uses, so duplicating it costs about twenty
//! lines rather than a new cross-family dependency.

use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list};

/// Which shape a list confidently is, for the purpose of deciding whether its
/// entries may be reordered.
enum Shape {
    /// `(:key value :key value ...)`: every key slot holds a keyword atom
    /// with no reader prefix, and there is no dangling trailing key.
    Plist,
    /// A run of `(key . value)` or `(key value)` entries, each a `(...)`
    /// list with an unprefixed atom in key position.
    Alist,
    Other,
}

fn shape(view: &ExpressionView) -> Shape {
    if !is_paren_list(view) || view.children.len() < 2 {
        return Shape::Other;
    }
    if is_plist_shaped(view) {
        Shape::Plist
    } else if is_alist_shaped(view) {
        Shape::Alist
    } else {
        Shape::Other
    }
}

fn is_plist_shaped(view: &ExpressionView) -> bool {
    let children = &view.children;
    if children.is_empty() || children.len() % 2 != 0 {
        return false;
    }
    let mut index = 0;
    while index < children.len() {
        let key = &children[index];
        if !key.reader_prefixes.is_empty() {
            return false;
        }
        if !atom_text(key).is_some_and(|text| text.starts_with(':')) {
            return false;
        }
        index += 2;
    }
    true
}

fn is_alist_shaped(view: &ExpressionView) -> bool {
    view.children
        .iter()
        .all(|child| alist_entry_key(child).is_some())
}

/// The key atom of one alist entry — `(key . value)` or `(key value)` — or
/// `None` when `child` is not confidently shaped like one: not a `(...)`
/// list, its own first child is not an atom, its key carries a reader
/// prefix, or its arity is anything other than those two.
fn alist_entry_key(child: &ExpressionView) -> Option<&ExpressionView> {
    if !child.reader_prefixes.is_empty() || !is_paren_list(child) {
        return None;
    }
    let key = child.children.first()?;
    if !key.reader_prefixes.is_empty() {
        return None;
    }
    atom_text(key)?;
    match child.children.len() {
        2 => Some(key),
        3 if is_dot(&child.children[1]) => Some(key),
        _ => None,
    }
}

/// Whether `view` is the `.` of a dotted pair — punctuation, not a value.
fn is_dot(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some(".")
}

/// Whether anything reachable in `view` — skipping every reader-prefixed
/// subtree, which is never touched — is confidently plist- or alist-shaped.
/// The gate for the whole command: nothing here means nothing to do.
fn contains_data_shape(view: &ExpressionView) -> bool {
    if !view.reader_prefixes.is_empty() {
        return false;
    }
    if matches!(shape(view), Shape::Plist | Shape::Alist) {
        return true;
    }
    view.children.iter().any(contains_data_shape)
}

/// `view`'s children, reordered by key when `view` is confidently plist- or
/// alist-shaped, in their original order otherwise.
///
/// The sort is by each key's own printed text, stably: `sort_by` on a `Vec`
/// never reorders two entries whose keys compare equal, so a duplicate key
/// keeps whichever entry was already first — this command corrects
/// whitespace and order, not a file's own duplicate-key mistakes, which
/// `inspect data-check` already reports separately.
fn reorder(view: &ExpressionView) -> Vec<&ExpressionView> {
    match shape(view) {
        Shape::Plist => {
            let mut pairs: Vec<[&ExpressionView; 2]> = view
                .children
                .chunks_exact(2)
                .map(|pair| [&pair[0], &pair[1]])
                .collect();
            pairs.sort_by(|a, b| atom_text(a[0]).cmp(&atom_text(b[0])));
            pairs.into_iter().flatten().collect()
        }
        Shape::Alist => {
            let mut entries: Vec<&ExpressionView> = view.children.iter().collect();
            entries.sort_by(|a, b| {
                alist_entry_key(a)
                    .and_then(atom_text)
                    .cmp(&alist_entry_key(b).and_then(atom_text))
            });
            entries
        }
        Shape::Other => view.children.iter().collect(),
    }
}

fn render(view: &ExpressionView, source: &str, out: &mut String) {
    if !view.reader_prefixes.is_empty() {
        out.push_str(&source[view.span.as_range()]);
        return;
    }
    match view.kind {
        ExpressionKind::Atom => out.push_str(&source[view.span.as_range()]),
        ExpressionKind::List => render_list(view, source, out),
        ExpressionKind::Root => {
            render_joined(&view.children.iter().collect::<Vec<_>>(), source, out);
        }
    }
}

/// A `(...)`/`[...]`/`{...}` list: its own two delimiter bytes, reordered
/// children in between. The delimiter characters are read straight off the
/// source at the span's own boundary bytes rather than reconstructed from
/// [`paredit_core_syntax::sexpr::Delimiter`], because that is simpler and
/// exactly as correct — every delimiter this parser accepts is one ASCII
/// byte, open and close alike.
fn render_list(view: &ExpressionView, source: &str, out: &mut String) {
    let start = view.span.start().get();
    let end = view.span.end().get();
    out.push_str(&source[start..start + 1]);
    let ordered = reorder(view);
    render_joined(&ordered, source, out);
    out.push_str(&source[end - 1..end]);
}

fn render_joined(children: &[&ExpressionView], source: &str, out: &mut String) {
    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        render(child, source, out);
    }
}

/// Rewrites `source` with every confidently alist- or plist-shaped list's
/// entries sorted by key and every list's own whitespace collapsed to a
/// single space between elements.
///
/// `None` when nothing in `tree` is confidently data-shaped: a file this
/// conservative about could not misjudge is a file this command should not
/// touch at all, rather than mangle whitespace for no benefit and no shape
/// to justify it.
#[must_use]
pub(super) fn canonicalize_document(source: &str, tree: &SyntaxTree) -> Option<String> {
    let root = tree.root_view();
    if !contains_data_shape(&root) {
        return None;
    }
    let mut rendered = String::new();
    render(&root, source, &mut rendered);
    rendered.push('\n');
    Some(rendered)
}

/// Every leaf token's exact source text, in tree order: an atom's own span,
/// or — copied whole rather than descended into — a reader-prefixed
/// subtree's span.
fn leaf_texts<'a>(view: &'a ExpressionView, source: &'a str, out: &mut Vec<&'a str>) {
    if !view.reader_prefixes.is_empty() {
        out.push(&source[view.span.as_range()]);
        return;
    }
    match view.kind {
        ExpressionKind::Atom => out.push(&source[view.span.as_range()]),
        ExpressionKind::List | ExpressionKind::Root => {
            for child in &view.children {
                leaf_texts(child, source, out);
            }
        }
    }
}

/// Whether `after` (parsed from `after_source`) carries exactly the same
/// multiset of leaf tokens as `before` (parsed from `before_source`) —
/// nothing added, dropped, or rewritten, whatever order they now appear in.
///
/// The belt to `render`'s own by-construction correctness: `render` never
/// invents or drops a leaf, so this should always hold, but a command that
/// writes files does not get to assume its own algorithm is bug-free —
/// reparsing and confirming equivalence before committing the write is the
/// same discipline `packages/core/cli/src/io.rs`'s reparse guard applies to
/// every other write in this codebase, extended to check more than "does it
/// still parse".
#[must_use]
pub(super) fn same_data(
    before_source: &str,
    before: &SyntaxTree,
    after_source: &str,
    after: &SyntaxTree,
) -> bool {
    let before_root = before.root_view();
    let mut before_leaves = Vec::new();
    leaf_texts(&before_root, before_source, &mut before_leaves);
    let after_root = after.root_view();
    let mut after_leaves = Vec::new();
    leaf_texts(&after_root, after_source, &mut after_leaves);
    before_leaves.sort_unstable();
    after_leaves.sort_unstable();
    before_leaves == after_leaves
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn canonicalize(source: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        canonicalize_document(source, &tree)
    }

    #[test]
    fn a_plist_is_sorted_by_key_with_single_space_whitespace() {
        let out = canonicalize("(:c   3\n  :a 1  :b 2)").expect("data-shaped");
        assert_eq!(out, "(:a 1 :b 2 :c 3)\n");
    }

    #[test]
    fn an_alist_is_sorted_by_key_preserving_each_entrys_own_shape() {
        let out = canonicalize("((c . 3) (a . 1)\n(b . 2))").expect("data-shaped");
        assert_eq!(out, "((a . 1) (b . 2) (c . 3))\n");
    }

    #[test]
    fn a_two_element_alist_entry_is_sorted_the_same_way() {
        let out = canonicalize("((c 3) (a 1) (b 2))").expect("data-shaped");
        assert_eq!(out, "((a 1) (b 2) (c 3))\n");
    }

    #[test]
    fn a_nested_plist_value_is_also_sorted() {
        let out = canonicalize("(:b (:z 1 :a 2) :a 1)").expect("data-shaped");
        assert_eq!(out, "(:a 1 :b (:a 2 :z 1))\n");
    }

    #[test]
    fn a_file_with_no_alist_or_plist_shape_anywhere_is_left_alone() {
        assert!(canonicalize("(defun add (a b) (+ a b))").is_none());
    }

    #[test]
    fn duplicate_keys_keep_their_relative_order() {
        let out = canonicalize("(:a 1 :b 2 :a 3)").expect("data-shaped");
        assert_eq!(out, "(:a 1 :a 3 :b 2)\n");
    }

    #[test]
    fn same_data_confirms_a_correct_rewrite() {
        let before =
            SyntaxTree::parse_with_dialect("(:b 2 :a 1)", Dialect::CommonLisp).expect("parse");
        let after =
            SyntaxTree::parse_with_dialect("(:a 1 :b 2)", Dialect::CommonLisp).expect("parse");
        assert!(same_data("(:b 2 :a 1)", &before, "(:a 1 :b 2)", &after));
    }

    #[test]
    fn same_data_catches_a_dropped_value() {
        let before =
            SyntaxTree::parse_with_dialect("(:b 2 :a 1)", Dialect::CommonLisp).expect("parse");
        let after =
            SyntaxTree::parse_with_dialect("(:a 1 :b)", Dialect::CommonLisp).expect("parse");
        assert!(!same_data("(:b 2 :a 1)", &before, "(:a 1 :b)", &after));
    }
}
