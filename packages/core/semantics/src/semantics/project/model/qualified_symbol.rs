//! Symbol identity across packages.

use std::fmt;

use paredit_core_syntax::sexpr::SymbolName;

/// A package's identity, normalized the way the Common Lisp reader would.
///
/// A package designator can be written as a symbol (`app`), a keyword
/// (`:app`), an uninterned symbol (`#:app`), or a string (`"app"`), and the
/// first three are case-folded while the last is not. Normalizing once, here,
/// is what lets `defpackage`, `in-package`, and a qualified reference agree
/// that they are talking about the same package.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId(String);

impl PackageId {
    /// The identity of a designator already stripped of its `:`/`#:`/quotes.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self(name.to_ascii_uppercase())
    }

    /// `COMMON-LISP`, which `CL` also names.
    #[must_use]
    pub fn common_lisp() -> Self {
        Self("COMMON-LISP".to_owned())
    }

    /// `KEYWORD`, the home of every `:foo`.
    #[must_use]
    pub fn keyword() -> Self {
        Self("KEYWORD".to_owned())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A symbol together with the package that owns it.
///
/// This is the identity the project layer exists to provide. Today's
/// cross-file analyses compare bare names, so `app:run` and `test:run` look
/// like the same definition; a report built on that will claim a change to one
/// affects callers of the other. Carrying the package makes the two
/// unmistakably different values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedSymbol {
    package: PackageId,
    name: SymbolName,
}

impl QualifiedSymbol {
    #[must_use]
    pub const fn new(package: PackageId, name: SymbolName) -> Self {
        Self { package, name }
    }

    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    #[must_use]
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }
}

impl fmt::Display for QualifiedSymbol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.package, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(text: &str) -> SymbolName {
        SymbolName::new(text).expect("symbol")
    }

    #[test]
    fn package_identity_folds_case() {
        assert_eq!(PackageId::new("app"), PackageId::new("APP"));
        assert_eq!(PackageId::new("My-App"), PackageId::new("MY-APP"));
    }

    #[test]
    fn the_standard_packages_have_their_canonical_names() {
        assert_eq!(PackageId::common_lisp().as_str(), "COMMON-LISP");
        assert_eq!(PackageId::keyword().as_str(), "KEYWORD");
        assert_eq!(PackageId::new("cl"), PackageId::new("CL"));
    }

    #[test]
    fn the_same_name_in_two_packages_is_two_symbols() {
        // The whole point of the layer: a name-only comparison would call
        // these equal and report a false cross-package impact.
        let left = QualifiedSymbol::new(PackageId::new("app"), symbol("run"));
        let right = QualifiedSymbol::new(PackageId::new("test"), symbol("run"));
        assert_ne!(left, right);
        assert_eq!(left.name(), right.name());
    }

    #[test]
    fn a_qualified_symbol_displays_in_reader_syntax() {
        let symbol = QualifiedSymbol::new(PackageId::new("app"), symbol("run"));
        assert_eq!(symbol.to_string(), "APP:run");
    }
}
