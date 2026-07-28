//! What each binding and each expression is known to be.

use std::collections::HashMap;

use crate::semantics::NodeKey;
use crate::semantics::binding::BindingId;

use super::ty::Ty;

/// A type, when one is provable.
///
/// Distinct from [`Ty::Top`]: `t` is the claim "this is some object", which is
/// true but useless, while `Unknown` is "this layer has no claim". Rules must
/// stay silent on `Unknown`; they may legitimately act on `Known(Top)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Known(Ty),
    Unknown,
}

impl Type {
    #[must_use]
    pub const fn as_known(self) -> Option<Ty> {
        match self {
            Self::Known(ty) => Some(ty),
            Self::Unknown => None,
        }
    }

    /// Whether this is provably a subtype of `bound`.
    #[must_use]
    pub fn is_definitely(self, bound: Ty) -> bool {
        self.as_known().is_some_and(|ty| ty.is_subtype_of(bound))
    }

    /// Whether this provably shares no member with `bound` — the question
    /// "can this argument possibly be a number?" asked in the safe direction.
    #[must_use]
    pub fn is_definitely_not(self, bound: Ty) -> bool {
        self.as_known()
            .is_some_and(|ty| super::lattice::meet(ty, bound) == Ty::Bottom)
    }
}

/// The types of one file's bindings and expressions.
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    bindings: HashMap<BindingId, Ty>,
    expressions: HashMap<NodeKey, Ty>,
}

impl TypeTable {
    pub fn binding_type(&self, binding: BindingId) -> Type {
        self.bindings
            .get(&binding)
            .copied()
            .map_or(Type::Unknown, Type::Known)
    }

    pub fn expression_type(&self, key: NodeKey) -> Type {
        self.expressions
            .get(&key)
            .copied()
            .map_or(Type::Unknown, Type::Known)
    }

    #[must_use]
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    #[must_use]
    pub fn expression_count(&self) -> usize {
        self.expressions.len()
    }

    /// Every binding this layer proved a type for.
    ///
    /// Unordered, because the backing map is. A report that prints these must
    /// impose its own order — by span, not by `BindingId` — or its output is
    /// not reproducible between runs.
    #[must_use]
    pub fn binding_types(&self) -> impl ExactSizeIterator<Item = (BindingId, Ty)> {
        self.bindings.iter().map(|(binding, ty)| (*binding, *ty))
    }

    /// Every expression this layer proved a type for. Unordered; see
    /// [`TypeTable::binding_types`].
    #[must_use]
    pub fn expression_types(&self) -> impl ExactSizeIterator<Item = (NodeKey, Ty)> {
        self.expressions.iter().map(|(key, ty)| (*key, *ty))
    }
}

/// A type table under construction.
#[derive(Debug, Default)]
pub struct TypeTableBuilder {
    bindings: HashMap<BindingId, Ty>,
    expressions: HashMap<NodeKey, Ty>,
}

impl TypeTableBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What a binding is known to be so far, for a source that must combine
    /// with it (flow narrowing meets this; a plain reference falls back to
    /// it) rather than blindly overwrite it.
    pub fn binding_type(&self, binding: BindingId) -> Type {
        self.bindings
            .get(&binding)
            .copied()
            .map_or(Type::Unknown, Type::Known)
    }

    /// Records what a binding is, narrowing against anything already known.
    ///
    /// Two sources agreeing narrows; two sources contradicting yields
    /// [`Ty::Bottom`], which is the honest record of an impossible
    /// declaration rather than a silent preference for one source.
    pub fn narrow_binding(&mut self, binding: BindingId, ty: Ty) {
        let narrowed = self
            .bindings
            .get(&binding)
            .map_or(ty, |known| super::lattice::meet(*known, ty));
        self.bindings.insert(binding, narrowed);
    }

    pub fn set_expression(&mut self, key: NodeKey, ty: Ty) {
        self.expressions.insert(key, ty);
    }

    #[must_use]
    pub fn finish(self) -> TypeTable {
        TypeTable {
            bindings: self.bindings,
            expressions: self.expressions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};

    fn key(start: usize, end: usize) -> NodeKey {
        NodeKey::atom(ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end)))
    }

    #[test]
    fn an_unrecorded_binding_or_expression_is_unknown() {
        let table = TypeTableBuilder::new().finish();
        assert_eq!(table.expression_type(key(0, 1)), Type::Unknown);
    }

    #[test]
    fn the_builder_reports_what_it_has_narrowed_so_far() {
        let mut builder = TypeTableBuilder::new();
        let binding = crate::semantics::binding::BindingId::new(0);
        assert_eq!(builder.binding_type(binding), Type::Unknown);
        builder.narrow_binding(binding, Ty::Number);
        assert_eq!(builder.binding_type(binding), Type::Known(Ty::Number));
    }

    #[test]
    fn unknown_makes_no_claim_in_either_direction() {
        assert!(!Type::Unknown.is_definitely(Ty::Number));
        assert!(!Type::Unknown.is_definitely_not(Ty::Number));
    }

    #[test]
    fn a_known_type_answers_both_directions() {
        let integer = Type::Known(Ty::Integer);
        assert!(integer.is_definitely(Ty::Number));
        assert!(integer.is_definitely_not(Ty::String));
        assert!(!integer.is_definitely_not(Ty::Number));
    }

    #[test]
    fn top_is_a_claim_while_unknown_is_not() {
        assert!(Type::Known(Ty::Top).is_definitely(Ty::Top));
        assert!(!Type::Unknown.is_definitely(Ty::Top));
    }

    #[test]
    fn agreeing_sources_narrow_and_contradicting_ones_empty_the_type() {
        let mut builder = TypeTableBuilder::new();
        let binding = crate::semantics::binding::BindingId::new(0);
        builder.narrow_binding(binding, Ty::Number);
        builder.narrow_binding(binding, Ty::Integer);
        assert_eq!(
            builder.finish().binding_type(binding),
            Type::Known(Ty::Integer)
        );

        let mut contradicting = TypeTableBuilder::new();
        contradicting.narrow_binding(binding, Ty::Integer);
        contradicting.narrow_binding(binding, Ty::String);
        assert_eq!(
            contradicting.finish().binding_type(binding),
            Type::Known(Ty::Bottom)
        );
    }
}
