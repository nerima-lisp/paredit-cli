//! The operator-to-rules index that makes one pass cheap.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::dialect::Dialect;

use super::ordering::RuleIndex;
use crate::domain::lint::model::HeadFilter;
use crate::domain::lint::registry::REGISTRY;

/// Rules grouped by what they want to see, built once for the process.
#[derive(Debug)]
pub struct HeadIndex {
    by_head: HashMap<&'static str, Vec<RuleIndex>>,
    all_nodes: Vec<RuleIndex>,
    whole_tree: Vec<RuleIndex>,
}

impl HeadIndex {
    /// The rules registered for a normalized head, or an empty slice.
    pub fn for_head(&self, head: &str) -> &[RuleIndex] {
        self.by_head.get(head).map_or(&[], Vec::as_slice)
    }

    /// Rules that examine every node regardless of operator.
    pub fn all_nodes(&self) -> &[RuleIndex] {
        &self.all_nodes
    }

    /// Rules handed the whole document once.
    pub fn whole_tree(&self) -> &[RuleIndex] {
        &self.whole_tree
    }
}

/// The process-wide index. Building it touches every registered rule once, so
/// it is cached rather than rebuilt per file.
pub fn head_index() -> &'static HeadIndex {
    static INDEX: OnceLock<HeadIndex> = OnceLock::new();
    INDEX.get_or_init(build)
}

fn build() -> HeadIndex {
    let mut by_head: HashMap<&'static str, Vec<RuleIndex>> = HashMap::new();
    let mut all_nodes = Vec::new();
    let mut whole_tree = Vec::new();

    for (position, entry) in REGISTRY.iter().enumerate() {
        let index = RuleIndex::new(u16::try_from(position).expect("registry fits in u16"));
        match entry.rule().head_filter() {
            HeadFilter::Heads(heads) => {
                for head in heads {
                    by_head.entry(head.as_str()).or_default().push(index);
                }
            }
            HeadFilter::AllNodes => all_nodes.push(index),
            HeadFilter::WholeTree => whole_tree.push(index),
        }
    }

    HeadIndex {
        by_head,
        all_nodes,
        whole_tree,
    }
}

/// The index key for a head as it appears in source.
///
/// The index is a pre-filter — a rule still applies its own head predicate — so
/// this must never be *narrower* than any rule's notion of "same operator", but
/// may be wider. Rules disagree on that notion today: some compare
/// `head.to_ascii_lowercase()`, others go through
/// `common_lisp_operator_head_eq` (which strips the four standard `cl:` aliases
/// and honours reader escapes). Folding the widest of those — strip any package
/// qualifier, resolve escapes, case-fold — is a superset of all of them, so a
/// rule can never be skipped for a head it would have accepted.
///
/// The overwhelmingly common shape (a plain lowercase symbol) is returned
/// borrowed, so the whole pass allocates nothing for ordinary code.
pub fn head_key(dialect: Dialect, head: &str) -> Cow<'_, str> {
    if dialect != Dialect::CommonLisp || is_already_folded(head) {
        return Cow::Borrowed(head);
    }
    Cow::Owned(common_lisp_symbol_reference_needle(head).to_ascii_lowercase())
}

/// Whether `head` is already its own key: no uppercase to fold, no package
/// qualifier or reader escape to resolve.
fn is_already_folded(head: &str) -> bool {
    !head
        .bytes()
        .any(|byte| byte.is_ascii_uppercase() || matches!(byte, b':' | b'|' | b'\\' | b'#'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(head: &str) -> String {
        head_key(Dialect::CommonLisp, head).into_owned()
    }

    #[test]
    fn a_plain_symbol_is_its_own_key_without_allocating() {
        assert!(matches!(
            head_key(Dialect::CommonLisp, "defun"),
            Cow::Borrowed("defun")
        ));
    }

    #[test]
    fn folds_case_and_package_qualifiers_onto_the_bare_name() {
        assert_eq!(key("MOD"), "mod");
        assert_eq!(key("cl:mod"), "mod");
        assert_eq!(key("CL:MOD"), "mod");
        assert_eq!(key("my-package:mod"), "mod");
        assert_eq!(key("#:mod"), "mod");
    }

    #[test]
    fn resolves_reader_escapes_conservatively() {
        // `|MOD|` denotes the same symbol as `mod`; `|mod|` does not, but a
        // wider key is safe because the rule re-checks the head itself.
        assert_eq!(key("|MOD|"), "mod");
        assert_eq!(key("|mod|"), "mod");
    }

    #[test]
    fn symbolic_operators_pass_through() {
        assert_eq!(key("/"), "/");
        assert_eq!(key("1+"), "1+");
        assert_eq!(key("<="), "<=");
    }

    #[test]
    fn a_non_common_lisp_head_is_left_exactly_as_written() {
        assert_eq!(head_key(Dialect::Clojure, "myNs/Fn"), "myNs/Fn");
    }
}
