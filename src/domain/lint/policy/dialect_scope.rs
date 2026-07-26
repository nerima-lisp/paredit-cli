//! Which dialects a lint rule is meaningful for.

use crate::domain::dialect::Dialect;

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

    /// A rule that also holds for other dialects. Nothing needs this yet —
    /// every shipped rule encodes CLHS semantics — but the value layer's
    /// dialect tables will.
    #[cfg(test)]
    pub const fn new(dialects: &'static [Dialect]) -> Self {
        Self(dialects)
    }

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
}
