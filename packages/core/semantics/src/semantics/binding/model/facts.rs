//! The properties of a binding that later layers gate on.

use paredit_core_syntax::sexpr::ByteSpan;

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
    #[must_use]
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
    #[must_use]
    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent)
    }

    /// Combines two observations; opacity is absorbing.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Transparent, Self::Transparent) => Self::Transparent,
            _ => Self::ContainsOpaqueRegion,
        }
    }
}

/// The kind of region that cost a scope its transparency.
///
/// [`ScopeOpacity`] is all that propagation gates on; this exists so a
/// measurement run can say *which* forms are costing the most resolution.
/// Widening the transparency tables without that evidence trades soundness for
/// a coverage number, which is the one thing this layer must not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpacityCauseKind {
    /// A list whose head names nothing any semantics table registers. This is
    /// the only cause that a table entry could ever remove, and the only one
    /// worth a histogram: the head might be a macro, and a macro can expand
    /// into an assignment that never appears in the source.
    UnknownHead,
    /// A list with nothing readable in head position — a computed head
    /// (`((lambda …) x)`), a prefixed atom, or the empty list. There is no
    /// name to register, so no table entry can help.
    UnreadableHead,
    /// Quoted data, `#.` read-eval, or `#'(…)`: text whose effect on a
    /// binding cannot be read off the source.
    QuotedOrReadTime,
    /// A `#+`/`#-` reader conditional or a `#n=`/`#n#` label, which decides at
    /// read time whether the text around it exists at all.
    ReaderDispatch,
    /// A binding form whose binding list has a shape the shared parser
    /// rejects, so which names it introduces is unknown.
    UnreadableBinderList,
}

impl OpacityCauseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnknownHead => "unknown head",
            Self::UnreadableHead => "unreadable head",
            Self::QuotedOrReadTime => "quoted or read-time",
            Self::ReaderDispatch => "reader dispatch",
            Self::UnreadableBinderList => "unreadable binder list",
        }
    }
}

/// Where a scope lost its transparency, and to what.
///
/// The site is a [`ByteSpan`] rather than an owned name because this is
/// recorded on the lint hot path, where roughly three of every four bindings
/// are opaque: a `String` per binding would be an allocation for a fact no
/// lint rule reads. Every caller that wants the text already holds the source
/// the span indexes into.
///
/// For [`OpacityCauseKind::UnknownHead`] the span is the head atom, so the
/// name can be sliced straight out. For every other kind it is the offending
/// form, since there is no name to point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpacityCause {
    kind: OpacityCauseKind,
    site: ByteSpan,
}

impl OpacityCause {
    #[must_use]
    pub const fn new(kind: OpacityCauseKind, site: ByteSpan) -> Self {
        Self { kind, site }
    }

    #[must_use]
    pub const fn kind(self) -> OpacityCauseKind {
        self.kind
    }

    #[must_use]
    pub const fn site(self) -> ByteSpan {
        self.site
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
