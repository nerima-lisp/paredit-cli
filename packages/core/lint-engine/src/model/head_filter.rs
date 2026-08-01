//! Which nodes of the single dispatch pass a rule wants to see.

/// A head spelling already reduced to the form the dispatcher's head index is
/// keyed by: package prefix stripped and ASCII-lowercased.
///
/// The index is only a pre-filter — every rule still re-checks the head itself
/// — so an over-approximation is harmless but an under-approximation silently
/// drops findings. Normalizing on both sides with the same rule is what makes
/// the two agree, and [`NormalizedHead::new`] is `const` so a rule that
/// declares `"cl:Defun"` fails to compile rather than failing to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalizedHead(&'static str);

impl NormalizedHead {
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        assert!(
            is_normalized(value),
            "a lint rule head must be non-empty, lowercase, and package-unqualified"
        );
        Self(value)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Non-empty, no ASCII uppercase, no package marker. Matches what
/// [`super::super::engine::head_index`] produces for a visited node.
const fn is_normalized(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_uppercase() || bytes[index] == b':' {
            return false;
        }
        index += 1;
    }
    true
}

/// The subset of the tree a rule is invoked on.
///
/// The dispatcher walks the document once; this is how a rule says which of
/// those nodes it wants, so that 100+ head-specific rules cost one hash lookup
/// per list node instead of one whole-tree walk each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadFilter {
    /// Only `(...)` lists whose normalized head is one of these.
    Heads(&'static [NormalizedHead]),
    /// Every node, including atoms — for rules keyed on shape, not operator.
    AllNodes,
    /// The whole document at once, for rules that need more than one node's
    /// worth of context and cannot be expressed as a per-node predicate.
    /// Eleven rules use this, for two unrelated reasons:
    ///
    /// - **Correlating separate top-level definitions.**
    ///   `duplicate-parameters`, `duplicate-lambda-list-keyword`, and
    ///   `lambda-list-keyword-order` all want the same narrower thing: the
    ///   *top-level* definitions only. `Heads` would be wrong for them,
    ///   because a `defun` inside an `flet` or a `let` is not a top-level
    ///   definition and picking it up would change what they report.
    ///
    ///   A `TopLevelHeads` variant was considered and declined for this case.
    ///   It would have to carry heads as well as the depth restriction, so it
    ///   is a whole additional dispatch mode; what it would replace is the
    ///   five lines below that hand each such rule the root view once per
    ///   *file*, not once per node. The three rules then walk `root_children`
    ///   themselves, which is explicit about the one thing that matters about
    ///   them. Adding a mode to remove a smaller one is not a simplification.
    ///
    /// - **Needing ancestor context a per-node predicate cannot see.**
    ///   `paredit-feature-lint-repl-debug`'s eight rules (`RuleContext`
    ///   carries no depth or parent, so `Heads`/`AllNodes` cannot answer "is
    ///   this call quoted data", "is it the last form of its enclosing body",
    ///   or "is its head shadowed by a local `flet`/`labels`/`macrolet`") each
    ///   run their own quote-aware, binding-aware walk from the root instead
    ///   — see `paredit_feature_lint_repl_debug::support`.
    ///
    /// Revisit if a rule in the first group's set changes, or if a rule needs
    /// per-node context this variant does not carry for some other reason.
    WholeTree,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normalized_head() {
        assert_eq!(NormalizedHead::new("defun").as_str(), "defun");
        assert_eq!(NormalizedHead::new("1+").as_str(), "1+");
        assert_eq!(NormalizedHead::new("/").as_str(), "/");
    }

    #[test]
    fn rejects_unnormalized_spellings() {
        assert!(!is_normalized(""));
        assert!(!is_normalized("Defun"));
        assert!(!is_normalized("cl:defun"));
    }
}
