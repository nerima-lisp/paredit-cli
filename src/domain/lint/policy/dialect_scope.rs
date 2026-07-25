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
pub enum RuleDialectScope {
    /// The overwhelming majority: CLHS-specific operator semantics.
    CommonLispOnly,
    /// A rule that also holds for the dialects listed.
    Dialects(&'static [Dialect]),
}

impl RuleDialectScope {
    pub fn includes(self, dialect: Dialect) -> bool {
        match self {
            Self::CommonLispOnly => dialect == Dialect::CommonLisp,
            Self::Dialects(dialects) => dialects.contains(&dialect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_only_excludes_every_other_dialect() {
        let scope = RuleDialectScope::CommonLispOnly;
        assert!(scope.includes(Dialect::CommonLisp));
        assert!(!scope.includes(Dialect::Clojure));
        assert!(!scope.includes(Dialect::EmacsLisp));
    }

    #[test]
    fn an_explicit_list_includes_exactly_its_members() {
        let scope = RuleDialectScope::Dialects(&[Dialect::CommonLisp, Dialect::EmacsLisp]);
        assert!(scope.includes(Dialect::EmacsLisp));
        assert!(!scope.includes(Dialect::Scheme));
    }
}
