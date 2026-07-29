//! Content-addressed ids that keep pointing at the same form after an edit.
//!
//! `--path 0.2.1` breaks the moment anything is inserted above it, which is
//! exactly what a multi-step refactor does: an agent resolves a path, applies
//! an edit, and every path it had cached is now off by one. A stable id is the
//! answer to "name this form in a way the next command still understands".
//!
//! # What the id is made of
//!
//! Three parts, hashed together:
//!
//! 1. **Context** — the nearest enclosing definition, as `defun/parse-header`.
//!    Two identical `(cleanup)` calls in two different functions get different
//!    ids, which is what makes the id useful rather than merely unique.
//! 2. **Shape** — the form's own source with whitespace collapsed
//!    ([`normalized_form_text`]). Reformatting does not move the id.
//! 3. **Ordinal** — the position among forms sharing the first two, in source
//!    order. Three identical `(incf n)` calls in one function are three ids.
//!
//! # What it deliberately does not survive
//!
//! Editing the form itself, renaming its enclosing definition, or adding an
//! earlier copy of an identical form. All three change what the id names, and
//! an id that silently followed such a change would be worse than one that
//! reports `no form carries selector id …` — which is a failure a caller can
//! recover from by re-resolving.
//!
//! The hash is FNV-1a/64, matching `stable_text_hash` in the CLI layer: this
//! is a lookup key inside one file, not a security boundary, and keeping the
//! two the same avoids adding a digest dependency to the syntax package.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::definition::definition_shape;
use crate::dialect::Dialect;
use crate::sexpr::{ExpressionKind, ExpressionPath, ExpressionView, SyntaxTree};
use crate::view_query::list_head;

use super::error::{SelectorError, SelectorResult};
use super::normalize::normalized_form_text;

/// How the id is spelled when a caller wants it unambiguous in a shell.
pub const STABLE_ID_PREFIX: &str = "sel:";

/// The number of hex characters an id carries.
const STABLE_ID_WIDTH: usize = 16;

/// One form's id together with the path it was computed for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableSelectorId {
    id: String,
}

impl StableSelectorId {
    /// The bare 16-character hex id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// Validates user input, accepting both `abcdef…` and `sel:abcdef…`.
    pub fn parse(value: &str) -> SelectorResult<Self> {
        let bare = value.strip_prefix(STABLE_ID_PREFIX).unwrap_or(value);
        let well_formed = bare.len() == STABLE_ID_WIDTH
            && bare
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
        if !well_formed {
            return Err(SelectorError::MalformedStableId {
                id: value.to_owned(),
            });
        }
        Ok(Self {
            id: bare.to_owned(),
        })
    }
}

impl std::fmt::Display for StableSelectorId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.id)
    }
}

/// Every form in `tree` paired with its stable id, in source order.
///
/// Built in one pass because the ordinal component needs to see the whole
/// file: an id cannot be computed for one form in isolation.
#[must_use]
pub fn stable_selector_ids(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> Vec<(ExpressionPath, StableSelectorId)> {
    let root = tree.root_view();
    let source = tree.source();
    let mut keys: Vec<(ExpressionPath, String)> = Vec::new();

    // Context labels are interned into an append-only arena and referred to by
    // index. A `&str` into the arena cannot be held across a later push, and
    // cloning the label into every child frame would allocate once per node
    // rather than once per definition.
    let mut arena: Vec<String> = Vec::new();
    // (view, path, index of the enclosing definition's label)
    let mut pending: Vec<(&ExpressionView, ExpressionPath, Option<usize>)> = root
        .children
        .iter()
        .enumerate()
        .rev()
        .map(|(index, child)| (child, ExpressionPath::root_child(index), None))
        .collect();

    while let Some((view, path, context)) = pending.pop() {
        let context_text = context.map_or("", |index| arena[index].as_str());
        keys.push((
            path.clone(),
            format!(
                "{context_text}\u{1f}{}",
                normalized_form_text(source, view.span)
            ),
        ));

        let child_context = match definition_context(view, dialect) {
            Some(label) => {
                arena.push(label);
                Some(arena.len() - 1)
            }
            None => context,
        };

        pending.extend(
            view.children
                .iter()
                .enumerate()
                .rev()
                .map(|(index, child)| (child, path.child(index), child_context)),
        );
    }

    assign_ordinals(keys)
}

/// `defun/parse-header` for a definition form, or `None` for anything else.
fn definition_context(view: &ExpressionView, dialect: Dialect) -> Option<String> {
    if view.kind != ExpressionKind::List {
        return None;
    }
    let head = list_head(view)?;
    let shape = definition_shape(dialect, view, head)?;
    let name = shape.name(view).unwrap_or("<anonymous>");
    Some(format!("{head}/{name}"))
}

fn assign_ordinals(keys: Vec<(ExpressionPath, String)>) -> Vec<(ExpressionPath, StableSelectorId)> {
    // A map rather than a scan: `keys` holds one entry per node, and a linear
    // lookup per node made this quadratic on a file with many similar forms.
    // Iteration order never leaves this function, so it stays deterministic.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut output = Vec::with_capacity(keys.len());

    for (path, key) in keys {
        let ordinal = match seen.entry(key.clone()) {
            Entry::Occupied(mut slot) => {
                *slot.get_mut() += 1;
                *slot.get()
            }
            Entry::Vacant(slot) => *slot.insert(0),
        };
        output.push((
            path,
            StableSelectorId {
                id: format!("{:016x}", fnv1a64(&format!("{key}\u{1f}{ordinal}"))),
            },
        ));
    }

    output.sort_by(|left, right| left.0.cmp(&right.0));
    output
}

/// The path carrying `id`, or a refusal naming the id that went stale.
pub fn resolve_stable_id(
    tree: &SyntaxTree,
    dialect: Dialect,
    id: &StableSelectorId,
) -> SelectorResult<ExpressionPath> {
    stable_selector_ids(tree, dialect)
        .into_iter()
        .find(|(_, candidate)| candidate == id)
        .map(|(path, _)| path)
        .ok_or_else(|| SelectorError::UnknownStableId { id: id.to_string() })
}

/// The id of one known path, if the path is in the tree.
#[must_use]
pub fn stable_id_for_path(
    tree: &SyntaxTree,
    dialect: Dialect,
    path: &ExpressionPath,
) -> Option<StableSelectorId> {
    stable_selector_ids(tree, dialect)
        .into_iter()
        .find(|(candidate, _)| candidate == path)
        .map(|(_, id)| id)
}

fn fnv1a64(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).unwrap()
    }

    fn id_of(source: &str, path: &str) -> String {
        let path = path.parse::<ExpressionPath>().unwrap();
        stable_id_for_path(&tree(source), Dialect::CommonLisp, &path)
            .unwrap()
            .to_string()
    }

    #[test]
    fn an_id_survives_an_insertion_above_the_form_it_names() {
        let before = "(defun f (x) (cleanup x))";
        let after = "(defvar *new* 1)\n(defun f (x) (cleanup x))";
        // The same form is at 0.3 before and 1.3 after.
        assert_eq!(id_of(before, "0.3"), id_of(after, "1.3"));
    }

    #[test]
    fn an_id_survives_a_reformat() {
        let dense = "(defun f (x) (cleanup x))";
        let spread = "(defun f (x)\n  (cleanup\n    x))";
        assert_eq!(id_of(dense, "0.3"), id_of(spread, "0.3"));
    }

    #[test]
    fn the_same_text_in_two_definitions_gets_two_ids() {
        let source = "(defun f () (cleanup)) (defun g () (cleanup))";
        assert_ne!(id_of(source, "0.2"), id_of(source, "1.2"));
    }

    #[test]
    fn identical_siblings_are_told_apart_by_ordinal() {
        let source = "(defun f () (incf n) (incf n))";
        assert_ne!(id_of(source, "0.2"), id_of(source, "0.3"));
    }

    #[test]
    fn resolving_an_id_returns_the_path_it_was_computed_from() {
        let source = "(defun f (x) (cleanup x))";
        let parsed = tree(source);
        let path = "0.3".parse::<ExpressionPath>().unwrap();
        let id = stable_id_for_path(&parsed, Dialect::CommonLisp, &path).unwrap();
        assert_eq!(
            resolve_stable_id(&parsed, Dialect::CommonLisp, &id).unwrap(),
            path
        );
    }

    #[test]
    fn a_stale_id_reports_itself_rather_than_guessing() {
        let parsed = tree("(defun f () nil)");
        let id = StableSelectorId::parse("sel:0123456789abcdef").unwrap();
        assert_eq!(
            resolve_stable_id(&parsed, Dialect::CommonLisp, &id)
                .unwrap_err()
                .to_string(),
            "no form carries selector id 0123456789abcdef"
        );
    }

    #[test]
    fn ids_are_accepted_with_or_without_the_prefix_and_refused_otherwise() {
        assert_eq!(
            StableSelectorId::parse("sel:0123456789abcdef")
                .unwrap()
                .as_str(),
            "0123456789abcdef"
        );
        assert_eq!(
            StableSelectorId::parse("0123456789abcdef")
                .unwrap()
                .as_str(),
            "0123456789abcdef"
        );
        assert!(StableSelectorId::parse("0123456789ABCDEF").is_err());
        assert!(StableSelectorId::parse("abc").is_err());
    }

    #[test]
    fn every_form_gets_a_distinct_id() {
        let source = "(defun f (x) (g x) (g x) (h (g x)))";
        let ids = stable_selector_ids(&tree(source), Dialect::CommonLisp);
        let mut unique = ids.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>();
        unique.sort_unstable();
        let total = unique.len();
        unique.dedup();
        assert_eq!(unique.len(), total, "ids collided within one file");
    }
}
