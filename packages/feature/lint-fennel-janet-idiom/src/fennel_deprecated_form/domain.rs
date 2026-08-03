//! `fennel-deprecated-form` detection: a special the Fennel reference lists
//! under "Deprecated Forms".
//!
//! The list is not a judgement call. Fennel's `reference.md` has a
//! `## Deprecated Forms` section, and every entry below is one of its
//! subsections; two of them also say so in the compiler's own doc metadata
//! (`specials.fnl:422`, `"Set name as a global with val. Deprecated."`).
//!
//! Each entry carries the replacement the reference itself names, because a
//! deprecation notice without the replacement is a rule that tells you to stop
//! and not what to do instead.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::head_symbol;

/// Fennel only. `global`, `require-macros` and `pick-args` all exist in other
/// dialects' vocabularies with unrelated meanings — Janet has no `global`
/// special at all, and a Clojure `require` is not this — so widening the scope
/// would report on code the reference says nothing about.
pub const DIALECTS: [Dialect; 1] = [Dialect::Fennel];

/// One deprecated special, with what the reference says to use instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deprecation {
    pub head: &'static str,
    pub replacement: &'static str,
    /// The reference subsection the deprecation is stated in.
    pub citation: &'static str,
}

/// Every special under `reference.md`'s "Deprecated Forms" heading.
///
/// `require-macros` and `pick-args` are the two other subsections of that
/// section; "Rest destructuring metamethod" is the fourth and is not a form, so
/// there is nothing to key a head on.
pub const DEPRECATIONS: [Deprecation; 3] = [
    Deprecation {
        head: "global",
        replacement: "a `local` in the module, or an explicit `(tset _G :name …)` if a true global is meant",
        citation: "reference.md, \"Deprecated Forms\" -> \"`global` set global variable\"",
    },
    Deprecation {
        head: "require-macros",
        replacement: "`import-macros`, which binds the macro module to a name",
        citation: "reference.md, \"Deprecated Forms\" -> \"`require-macros` load macros with less flexibility\"",
    },
    Deprecation {
        head: "pick-args",
        replacement: "a `fn` with the arity written out, or `#(f $1 $2)`",
        citation: "reference.md, \"Deprecated Forms\" -> \"`pick-args` create a function of fixed arity\"",
    },
];

/// The deprecation for `head`, if it names one.
#[must_use]
pub fn deprecation_for(head: &str) -> Option<Deprecation> {
    DEPRECATIONS
        .iter()
        .copied()
        .find(|entry| entry.head == head)
}

/// One use of a deprecated special.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedUse {
    pub span: ByteSpan,
    pub deprecation: Deprecation,
}

/// Examines one form.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<DeprecatedUse> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let head = head_symbol(view)?;
    // A bare `(global)` with no name is malformed and the compiler says so;
    // this rule is about a form that works and should not be written.
    if view.children.len() < 2 {
        return None;
    }
    deprecation_for(head).map(|deprecation| DeprecatedUse {
        span: view.span,
        deprecation,
    })
}

/// Every deprecated form in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<DeprecatedUse> {
    let root = tree.root_view();
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// How many forms this rule could have looked at: every `(head …)` call whose
/// head is one of the three, before the arity guard. The denominator a
/// zero-finding sweep needs.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| deprecation_for(head).is_some()) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heads(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        collect(dialect, &tree)
            .into_iter()
            .map(|item| item.deprecation.head)
            .collect()
    }

    #[test]
    fn flags_every_deprecated_special() {
        assert_eq!(heads("(global x 1)", Dialect::Fennel), vec!["global"]);
        assert_eq!(
            heads("(require-macros :my.macros)", Dialect::Fennel),
            vec!["require-macros"]
        );
        assert_eq!(heads("(pick-args 2 f)", Dialect::Fennel), vec!["pick-args"]);
    }

    #[test]
    fn names_the_replacement_the_reference_names() {
        assert!(
            deprecation_for("require-macros")
                .expect("entry")
                .replacement
                .contains("import-macros")
        );
    }

    #[test]
    fn leaves_the_supported_spellings_alone() {
        assert!(heads("(local x 1)", Dialect::Fennel).is_empty());
        assert!(heads("(import-macros m :my.macros)", Dialect::Fennel).is_empty());
        assert!(heads("(fn f [a b] (g a b))", Dialect::Fennel).is_empty());
    }

    #[test]
    fn a_head_that_merely_starts_the_same_is_not_one() {
        assert!(heads("(globalize x)", Dialect::Fennel).is_empty());
        assert!(heads("(require :socket)", Dialect::Fennel).is_empty());
    }

    #[test]
    fn a_nested_use_is_reached() {
        assert_eq!(
            heads("(fn setup []\n  (global cache {}))", Dialect::Fennel),
            vec!["global"]
        );
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // Janet has no `global` special; Clojure's `require` is unrelated.
        assert!(heads("(global x 1)", Dialect::Janet).is_empty());
        assert!(heads("(global x 1)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn the_candidate_count_counts_the_forms_looked_at() {
        let tree =
            SyntaxTree::parse_with_dialect("(global x 1) (global) (local y 2)", Dialect::Fennel)
                .expect("parse");
        assert_eq!(candidate_count(Dialect::Fennel, &tree), 2);
        assert_eq!(collect(Dialect::Fennel, &tree).len(), 1);
    }
}
