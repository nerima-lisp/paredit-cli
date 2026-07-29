//! The tree diff: what changed between two S-expression documents, in terms of
//! forms rather than lines.
//!
//! A text diff of Lisp answers a question nobody asked. Re-indenting a `let`
//! rewrites every line under it; moving a `defun` between files shows as a
//! deletion and an unrelated insertion; adding one argument to a call reports
//! the whole wrapped line. None of those are changes to the program, and all of
//! them cost a reviewer attention.
//!
//! This compares the parse instead. Two documents are the same here when they
//! have the same forms in the same order with the same atoms — whitespace,
//! indentation, and comments are not part of the comparison, because they are
//! not part of the tree. What comes out is a list of *form-level* edits, each
//! naming the path it happened at.
//!
//! ## What the comparison is blind to
//!
//! Comments and whitespace, by construction. That is the feature — a
//! reformatted file diffs as empty — and it is also the limit: this cannot be
//! the only diff a change is reviewed through, because a comment that now
//! contradicts the code it sits above is invisible to it. The CLI says so in
//! its summary rather than leaving the caller to infer it.

use std::collections::BTreeMap;

use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

/// What happened to one form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// A form present in the new document and not in the old.
    Inserted,
    /// A form present in the old document and not in the new.
    Deleted,
    /// A form in both, whose contents differ irreconcilably at this level.
    Replaced,
}

impl ChangeKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inserted => "inserted",
            Self::Deleted => "deleted",
            Self::Replaced => "replaced",
        }
    }
}

/// One side of a change: a form's span in its own document, and its text.
#[derive(Debug, Clone)]
pub struct Excerpt {
    pub span: ByteSpan,
    pub text: String,
}

/// How a change can be carried to a third file: a form to find, and what it
/// becomes.
///
/// The change's own "before" side is not always usable as an anchor. A change
/// to a bare atom — `car` becoming `first` — names a token, not a place, and
/// searching a third file for the token `car` finds every one of them. An
/// insertion has no before side at all. In both cases the *enclosing form* is a
/// real anchor: `(car (reverse xs))` occurs where the change belongs and not
/// where it does not, and replacing it wholesale carries the edit.
///
/// So the anchor is the change's own before side when that is a list, and the
/// enclosing form otherwise. `widened` records which, because a caller reading
/// a patch plan should be able to see that a change was matched by more context
/// than it names.
#[derive(Debug, Clone)]
pub struct Portable {
    pub anchor: String,
    pub replacement: String,
    pub widened: bool,
}

/// One form-level edit.
#[derive(Debug, Clone)]
pub struct Change {
    pub kind: ChangeKind,
    /// The dotted child-index path of the affected form, in the document that
    /// has it. For an insertion this is the path in the *new* document, since
    /// the old one has nothing there to name.
    pub path: String,
    /// How deep the change is. `0` is a whole top-level form; a larger number
    /// means the diff descended into a form rather than replacing it wholesale,
    /// which is what makes the result narrower than a text diff.
    pub depth: usize,
    pub before: Option<Excerpt>,
    pub after: Option<Excerpt>,
    /// How to carry this change elsewhere, or `None` when nothing anchors it —
    /// a top-level insertion, which names no existing form on either side.
    pub portable: Option<Portable>,
}

impl Change {
    /// The head symbol of the form the change touches, when it has one.
    ///
    /// This is what makes a change readable at a glance — `defun`, `let`,
    /// `handler-case` — and what `refactor patch` matches on when deciding
    /// whether a change is portable.
    #[must_use]
    pub fn head(&self) -> Option<String> {
        self.after
            .as_ref()
            .or(self.before.as_ref())
            .and_then(|excerpt| head_symbol(&excerpt.text))
    }
}

fn head_symbol(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix(['(', '[', '{'])?;
    let head: String = inner
        .chars()
        .take_while(|character| {
            !character.is_whitespace() && !matches!(character, '(' | ')' | '[' | ']' | '{' | '}')
        })
        .collect();
    (!head.is_empty()).then_some(head)
}

/// A content hash of an expression's structure.
///
/// Two expressions share a hash exactly when they have the same shape and the
/// same atoms, whatever the whitespace between them. Reader prefixes are part
/// of the hash: `'x` and `x` are different programs, and a diff that called
/// them equal would hide the change that matters most about a quote.
#[must_use]
pub fn shape_hash(view: &ExpressionView) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hash_into(view, &mut hasher);
    *hasher.finalize().as_bytes()
}

fn hash_into(view: &ExpressionView, hasher: &mut blake3::Hasher) {
    for prefix in &view.reader_prefixes {
        hasher.update(prefix.as_source().as_bytes());
    }
    match view.kind {
        ExpressionKind::Atom => {
            hasher.update(b"a\x00");
            hasher.update(view.text.as_deref().unwrap_or_default().as_bytes());
        }
        ExpressionKind::List | ExpressionKind::Root => {
            hasher.update(b"l\x00");
            hasher.update(&[delimiter_byte(view.delimiter)]);
            for child in &view.children {
                hash_into(child, hasher);
            }
        }
    }
    hasher.update(b"\x00");
}

/// The delimiter's opening character, or `0` for the root.
///
/// Spelled here rather than taken from `Delimiter` because the type's `open`
/// accessor is internal to the syntax package; a byte per variant is the whole
/// need, and matching exhaustively means a fourth delimiter would not silently
/// hash as one of the first three.
const fn delimiter_byte(delimiter: Option<Delimiter>) -> u8 {
    match delimiter {
        Some(Delimiter::Paren) => b'(',
        Some(Delimiter::Bracket) => b'[',
        Some(Delimiter::Brace) => b'{',
        None => 0,
    }
}

/// Diffs two parsed documents.
///
/// `old_source` and `new_source` are the texts the views were parsed from; the
/// views carry spans into them, not the text itself.
#[must_use]
pub fn diff_documents(
    old: &ExpressionView,
    old_source: &str,
    new: &ExpressionView,
    new_source: &str,
) -> Vec<Change> {
    let mut changes = Vec::new();
    diff_children(
        old,
        old_source,
        new,
        new_source,
        &Context {
            path: Vec::new(),
            depth: 0,
        },
        &mut changes,
    );
    changes
}

struct Context {
    path: Vec<usize>,
    depth: usize,
}

impl Context {
    fn child(&self, index: usize) -> Self {
        let mut path = self.path.clone();
        path.push(index);
        Self {
            path,
            depth: self.depth + 1,
        }
    }

    fn dotted(&self) -> String {
        self.path
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// Compares one node against its counterpart.
///
/// Descends when both sides are lists opened by the same delimiter, so a change
/// inside a `defun` is reported as the changed sub-form and not as the whole
/// definition. Anything else — an atom against a list, a paren against a
/// bracket — is a replacement, because there is no correspondence to descend
/// through.
fn diff_view(
    old: &ExpressionView,
    old_source: &str,
    new: &ExpressionView,
    new_source: &str,
    context: &Context,
    parents: Parents<'_>,
    changes: &mut Vec<Change>,
) {
    if shape_hash(old) == shape_hash(new) {
        return;
    }

    let both_lists = old.kind != ExpressionKind::Atom && new.kind != ExpressionKind::Atom;
    if both_lists && old.delimiter == new.delimiter && old.reader_prefixes == new.reader_prefixes {
        diff_children(old, old_source, new, new_source, context, changes);
        return;
    }

    changes.push(Change {
        kind: ChangeKind::Replaced,
        path: context.dotted(),
        depth: context.depth,
        before: Some(excerpt(old, old_source)),
        after: Some(excerpt(new, new_source)),
        portable: portable(
            Some(old),
            Some(new),
            parents,
            Sources {
                old: old_source,
                new: new_source,
            },
        ),
    });
}

/// The corresponding enclosing forms, or `None` at the top level.
type Parents<'a> = Option<(&'a ExpressionView, &'a ExpressionView)>;

#[derive(Clone, Copy)]
struct Sources<'a> {
    old: &'a str,
    new: &'a str,
}

/// Chooses the form a change can be found by in a third file.
///
/// A list on the before side is its own anchor. Anything else — a bare atom, or
/// an insertion with no before side at all — widens to the enclosing form,
/// which is a real place where the token alone is not. At the top level there
/// is nothing to widen to, and the change is not portable.
fn portable(
    before: Option<&ExpressionView>,
    after: Option<&ExpressionView>,
    parents: Parents<'_>,
    sources: Sources<'_>,
) -> Option<Portable> {
    if let Some(before) = before {
        if before.kind != ExpressionKind::Atom {
            return Some(Portable {
                anchor: excerpt(before, sources.old).text,
                replacement: after
                    .map(|after| excerpt(after, sources.new).text)
                    .unwrap_or_default(),
                widened: false,
            });
        }
    }

    let (old_parent, new_parent) = parents?;
    if old_parent.kind == ExpressionKind::Root {
        return None;
    }
    Some(Portable {
        anchor: excerpt(old_parent, sources.old).text,
        replacement: excerpt(new_parent, sources.new).text,
        widened: true,
    })
}

/// Aligns two child lists and reports what did not line up.
///
/// The alignment is a longest common subsequence over the children's shape
/// hashes: it finds the forms that survived unchanged, and everything between
/// two survivors is a region that changed. Within such a region the leftovers
/// are paired positionally and compared, so an edited form is reported as an
/// edit rather than as a delete plus an unrelated insert; a surplus on either
/// side is the insertion or deletion.
fn diff_children(
    old: &ExpressionView,
    old_source: &str,
    new: &ExpressionView,
    new_source: &str,
    context: &Context,
    changes: &mut Vec<Change>,
) {
    let old_hashes: Vec<[u8; 32]> = old.children.iter().map(shape_hash).collect();
    let new_hashes: Vec<[u8; 32]> = new.children.iter().map(shape_hash).collect();
    let matched = longest_common_subsequence(&old_hashes, &new_hashes);

    let mut old_index = 0;
    let mut new_index = 0;
    // A sentinel past the end closes the final region without a second copy of
    // the pairing logic below.
    let anchors = matched
        .iter()
        .copied()
        .chain(std::iter::once((old.children.len(), new.children.len())));

    for (old_anchor, new_anchor) in anchors {
        pair_region(
            RegionSides {
                old,
                old_source,
                old_range: old_index..old_anchor,
                new,
                new_source,
                new_range: new_index..new_anchor,
            },
            context,
            changes,
        );
        old_index = old_anchor + 1;
        new_index = new_anchor + 1;
    }
}

struct RegionSides<'a> {
    old: &'a ExpressionView,
    old_source: &'a str,
    old_range: std::ops::Range<usize>,
    new: &'a ExpressionView,
    new_source: &'a str,
    new_range: std::ops::Range<usize>,
}

fn pair_region(sides: RegionSides<'_>, context: &Context, changes: &mut Vec<Change>) {
    let RegionSides {
        old,
        old_source,
        old_range,
        new,
        new_source,
        new_range,
    } = sides;
    let old_positions: Vec<usize> = old_range.collect();
    let new_positions: Vec<usize> = new_range.collect();

    let sources = Sources {
        old: old_source,
        new: new_source,
    };
    let parents = Some((old, new));

    for (&old_position, &new_position) in old_positions.iter().zip(new_positions.iter()) {
        diff_view(
            &old.children[old_position],
            old_source,
            &new.children[new_position],
            new_source,
            &context.child(old_position),
            parents,
            changes,
        );
    }

    let paired = old_positions.len().min(new_positions.len());
    for &position in &old_positions[paired..] {
        changes.push(Change {
            kind: ChangeKind::Deleted,
            path: context.child(position).dotted(),
            depth: context.depth + 1,
            before: Some(excerpt(&old.children[position], old_source)),
            after: None,
            portable: portable(Some(&old.children[position]), None, parents, sources),
        });
    }
    for &position in &new_positions[paired..] {
        changes.push(Change {
            kind: ChangeKind::Inserted,
            path: context.child(position).dotted(),
            depth: context.depth + 1,
            before: None,
            after: Some(excerpt(&new.children[position], new_source)),
            portable: portable(None, Some(&new.children[position]), parents, sources),
        });
    }
}

fn excerpt(view: &ExpressionView, source: &str) -> Excerpt {
    Excerpt {
        span: view.span,
        text: source
            .get(view.span.start().get()..view.span.end().get())
            .unwrap_or_default()
            .to_owned(),
    }
}

/// The index pairs of a longest common subsequence of two hash sequences.
///
/// The classic quadratic dynamic program. The sequences here are one node's
/// children — tens of items for a form, hundreds for a whole file's top level —
/// so the quadratic table is small, and a linear-space or Myers implementation
/// would buy nothing but a harder function to check.
fn longest_common_subsequence(old: &[[u8; 32]], new: &[[u8; 32]]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0_usize; new.len() + 1]; old.len() + 1];
    for (old_index, old_hash) in old.iter().enumerate().rev() {
        for (new_index, new_hash) in new.iter().enumerate().rev() {
            lengths[old_index][new_index] = if old_hash == new_hash {
                lengths[old_index + 1][new_index + 1] + 1
            } else {
                lengths[old_index + 1][new_index].max(lengths[old_index][new_index + 1])
            };
        }
    }

    let mut pairs = Vec::new();
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() && new_index < new.len() {
        if old[old_index] == new[new_index] {
            pairs.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if lengths[old_index + 1][new_index] >= lengths[old_index][new_index + 1] {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    pairs
}

/// Every subtree of a document, keyed by shape hash.
///
/// `refactor patch` needs this to find where a change's "before" side occurs in
/// a third file. Values are lists because the same form can appear more than
/// once, and a patch that silently picked the first occurrence would be a
/// rewrite the caller did not ask for.
#[must_use]
pub fn index_subtrees(view: &ExpressionView) -> BTreeMap<[u8; 32], Vec<ByteSpan>> {
    let mut index: BTreeMap<[u8; 32], Vec<ByteSpan>> = BTreeMap::new();
    let mut stack = vec![view];
    while let Some(node) = stack.pop() {
        if node.kind != ExpressionKind::Root {
            index.entry(shape_hash(node)).or_default().push(node.span);
        }
        stack.extend(node.children.iter());
    }
    for spans in index.values_mut() {
        spans.sort_by_key(|span| (span.start().get(), span.end().get()));
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn diff(old: &str, new: &str) -> Vec<Change> {
        let old_tree = SyntaxTree::parse(old).expect("old parses");
        let new_tree = SyntaxTree::parse(new).expect("new parses");
        diff_documents(&old_tree.root_view(), old, &new_tree.root_view(), new)
    }

    fn kinds(changes: &[Change]) -> Vec<&'static str> {
        changes.iter().map(|change| change.kind.label()).collect()
    }

    #[test]
    fn reformatting_is_not_a_change() {
        let changes = diff("(defun f (x) (+ x 1))", "(defun f (x)\n  (+ x\n     1))\n");
        assert!(changes.is_empty(), "{changes:?}");
    }

    #[test]
    fn a_comment_is_not_a_change() {
        let changes = diff("(defun f (x) x)", ";; a note\n(defun f (x) x)\n");
        assert!(changes.is_empty(), "{changes:?}");
    }

    /// A quote is part of the program, so it must survive into the hash.
    #[test]
    fn adding_a_quote_is_a_change() {
        let changes = diff("(list x)", "(list 'x)");
        assert_eq!(kinds(&changes), vec!["replaced"]);
    }

    /// The point of descending: one edited argument reports as that argument,
    /// not as the whole definition it sits in.
    #[test]
    fn an_edit_deep_inside_a_form_is_reported_at_its_own_depth() {
        let changes = diff("(defun f (x) (+ x 1))", "(defun f (x) (+ x 2))");
        assert_eq!(kinds(&changes), vec!["replaced"]);
        assert_eq!(changes[0].path, "0.3.2");
        assert!(changes[0].depth > 1, "{:?}", changes[0]);
        assert_eq!(changes[0].before.as_ref().expect("before").text, "1");
        assert_eq!(changes[0].after.as_ref().expect("after").text, "2");
    }

    #[test]
    fn an_added_argument_is_an_insertion_rather_than_a_replacement() {
        let changes = diff("(f a b)", "(f a b c)");
        assert_eq!(kinds(&changes), vec!["inserted"]);
        assert_eq!(changes[0].after.as_ref().expect("after").text, "c");
    }

    /// An unchanged definition between two edited ones must not be dragged in.
    /// This is what the common-subsequence alignment buys over pairing by
    /// index.
    #[test]
    fn an_unchanged_form_survives_an_insertion_above_it() {
        let changes = diff(
            "(defun a () 1)\n(defun b () 2)\n",
            "(defun z () 0)\n(defun a () 1)\n(defun b () 2)\n",
        );
        assert_eq!(kinds(&changes), vec!["inserted"]);
        assert_eq!(
            changes[0].after.as_ref().expect("after").text,
            "(defun z () 0)"
        );
    }

    #[test]
    fn a_removed_definition_is_a_deletion() {
        let changes = diff("(defun a () 1)\n(defun b () 2)\n", "(defun b () 2)\n");
        assert_eq!(kinds(&changes), vec!["deleted"]);
        assert_eq!(
            changes[0].before.as_ref().expect("before").text,
            "(defun a () 1)"
        );
    }

    /// A bracket is not a paren. Descending across the two would report the
    /// contents as unchanged and lose the delimiter change entirely.
    #[test]
    fn a_changed_delimiter_replaces_rather_than_descends() {
        let changes = diff("(let ((x 1)) x)", "(let [(x 1)] x)");
        assert_eq!(kinds(&changes), vec!["replaced"]);
    }

    #[test]
    fn the_head_symbol_is_reported_for_a_list_change() {
        let changes = diff("(defun a () 1)", "(defun a () 2)\n(defmacro m () nil)");
        let heads: Vec<Option<String>> = changes.iter().map(Change::head).collect();
        assert!(heads.contains(&Some("defmacro".to_owned())), "{heads:?}");
    }

    #[test]
    fn subtree_index_records_every_occurrence_of_a_repeated_form() {
        let source = "(f (g 1)) (h (g 1))";
        let tree = SyntaxTree::parse(source).expect("parses");
        let index = index_subtrees(&tree.root_view());
        let inner = SyntaxTree::parse("(g 1)").expect("parses");
        let hash = shape_hash(&inner.root_view().children[0]);
        assert_eq!(index[&hash].len(), 2);
    }
}
