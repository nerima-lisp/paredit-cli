//! The properties of a binding that later layers gate on.

/// What kind of thing a name is bound to.
///
/// The namespaces are separate in Common Lisp: `(flet ((x ...)) x)` references
/// a *variable* `x`, not the local function. Collapsing them would make the
/// value layer propagate a function's definition into a variable reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKind {
    /// `let`/`let*`/`do`/`loop`/lambda lists/destructuring patterns.
    Variable,
    /// `flet`/`labels`/`defun` and friends.
    Function,
    /// `macrolet`.
    Macro,
    /// `symbol-macrolet`.
    SymbolMacro,
    /// A structure or class slot name.
    Slot,
}

/// Whether a binding has dynamic extent.
///
/// A special binding is visible to every call in the body's dynamic extent
/// with no textual reference anywhere, so nothing lexical can be concluded
/// about its value. Detection goes through the existing declaration scan
/// (`declaim`/`proclaim`/`defvar`/`defparameter`/`declare special`) and
/// deliberately **not** through the earmuff naming convention: a convention is
/// not a proof, and the semantic layers only mark facts they can prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialBinding {
    Lexical,
    DeclaredSpecial,
}

impl SpecialBinding {
    pub const fn is_lexical(self) -> bool {
        matches!(self, Self::Lexical)
    }
}

/// Whether anything inside a binding's scope is beyond static reach.
///
/// An unknown macro's expansion, quoted data, and reader-conditional text can
/// all do arbitrary things to a binding that no traversal can see. Recording
/// that a scope contains such a region lets the value layer refuse to
/// propagate rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeOpacity {
    /// Every form in the scope has known semantics.
    Transparent,
    /// The scope contains an unknown macro call, quoted data, or a
    /// reader-conditional region.
    ContainsOpaqueRegion,
}

impl ScopeOpacity {
    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent)
    }

    /// Combines two observations; opacity is absorbing.
    pub const fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Transparent, Self::Transparent) => Self::Transparent,
            _ => Self::ContainsOpaqueRegion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_is_absorbing() {
        assert_eq!(
            ScopeOpacity::Transparent.join(ScopeOpacity::Transparent),
            ScopeOpacity::Transparent
        );
        assert_eq!(
            ScopeOpacity::Transparent.join(ScopeOpacity::ContainsOpaqueRegion),
            ScopeOpacity::ContainsOpaqueRegion
        );
        assert_eq!(
            ScopeOpacity::ContainsOpaqueRegion.join(ScopeOpacity::Transparent),
            ScopeOpacity::ContainsOpaqueRegion
        );
    }

    #[test]
    fn only_a_lexical_binding_reads_as_lexical() {
        assert!(SpecialBinding::Lexical.is_lexical());
        assert!(!SpecialBinding::DeclaredSpecial.is_lexical());
    }
}
