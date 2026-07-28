//! Which dialects a lint rule is meaningful for.

use paredit_core_syntax::dialect::Dialect;

/// The dialects a rule applies to.
///
/// Every rule used to open its `collect_*` with the same
/// `if dialect != Dialect::CommonLisp { return empty }` guard. Stating the
/// scope as data instead lets the dispatcher skip a rule before walking
/// anything, and makes "this rule is Common Lisp only" a declaration rather
/// than an early return that is easy to forget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleDialectScope(&'static [Dialect]);

impl RuleDialectScope {
    /// The overwhelming majority: CLHS-specific operator semantics.
    pub const COMMON_LISP_ONLY: Self = Self(&[Dialect::CommonLisp]);

    /// Rules that encode Clojure semantics, which share almost no operator
    /// vocabulary with the CLHS ones.
    pub const CLOJURE_ONLY: Self = Self(&[Dialect::Clojure]);

    /// A rule that holds for an arbitrary set of dialects.
    #[must_use]
    pub const fn new(dialects: &'static [Dialect]) -> Self {
        Self(dialects)
    }

    #[must_use]
    pub fn includes(self, dialect: Dialect) -> bool {
        self.0.contains(&dialect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_only_excludes_every_other_dialect() {
        let scope = RuleDialectScope::COMMON_LISP_ONLY;
        assert!(scope.includes(Dialect::CommonLisp));
        assert!(!scope.includes(Dialect::Clojure));
        assert!(!scope.includes(Dialect::EmacsLisp));
    }

    #[test]
    fn an_explicit_list_includes_exactly_its_members() {
        let scope = RuleDialectScope::new(&[Dialect::CommonLisp, Dialect::EmacsLisp]);
        assert!(scope.includes(Dialect::EmacsLisp));
        assert!(!scope.includes(Dialect::Scheme));
    }

    #[test]
    fn clojure_only_and_common_lisp_only_are_disjoint() {
        assert!(RuleDialectScope::CLOJURE_ONLY.includes(Dialect::Clojure));
        assert!(!RuleDialectScope::CLOJURE_ONLY.includes(Dialect::CommonLisp));
        assert!(!RuleDialectScope::COMMON_LISP_ONLY.includes(Dialect::Clojure));
    }
}
