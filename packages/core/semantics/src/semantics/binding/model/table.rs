//! The per-file binding table and the scope tree it hangs on.

use std::collections::HashMap;

use paredit_core_syntax::sexpr::ByteSpan;

use crate::semantics::NodeKey;

use super::binding::{Binding, BindingDraft};
use super::ids::{BindingId, ScopeId};

/// One lexical scope. The file scope is the root and has no parent.
#[derive(Debug, Clone)]
pub struct Scope {
    parent: Option<ScopeId>,
    /// The form that opened the scope, or `None` for the file scope.
    opener: Option<ByteSpan>,
}

impl Scope {
    #[must_use]
    pub const fn parent(&self) -> Option<ScopeId> {
        self.parent
    }

    #[must_use]
    pub const fn opener(&self) -> Option<ByteSpan> {
        self.opener
    }
}

/// Every binding in one file, plus the map from a reference's position to the
/// binding it resolves to.
///
/// Built once per file rather than queried once per symbol. That inversion is
/// what the value and type layers need: they must ask "what does the name at
/// *this* position mean", which a per-symbol query cannot answer without
/// already knowing the symbol.
///
/// Only *resolvable* references are in [`BindingTable::resolution`]. Free
/// variables, globals, and anything inside an opaque region are absent — the
/// table never guesses, so a lookup miss means "not statically known", not
/// "no such name".
#[derive(Debug, Clone)]
pub struct BindingTable {
    scopes: Vec<Scope>,
    bindings: Vec<Binding>,
    resolution: HashMap<NodeKey, BindingId>,
}

impl BindingTable {
    #[must_use]
    pub fn binding(&self, id: BindingId) -> &Binding {
        &self.bindings[id.index()]
    }

    #[must_use]
    pub fn scope(&self, id: ScopeId) -> &Scope {
        &self.scopes[id.index()]
    }

    #[must_use]
    pub fn bindings(&self) -> impl ExactSizeIterator<Item = (BindingId, &Binding)> {
        self.bindings
            .iter()
            .enumerate()
            .map(|(index, binding)| (BindingId::new(index as u32), binding))
    }

    #[must_use]
    pub fn scope_count(&self) -> usize {
        self.scopes.len()
    }

    /// The binding a reference at `key` resolves to, or `None` when it is not
    /// statically resolvable.
    #[must_use]
    pub fn resolve(&self, key: NodeKey) -> Option<BindingId> {
        self.resolution.get(&key).copied()
    }

    /// Whether `scope` is `ancestor` or is lexically nested inside it.
    #[must_use]
    pub fn is_within(&self, scope: ScopeId, ancestor: ScopeId) -> bool {
        let mut current = Some(scope);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.scope(id).parent();
        }
        false
    }
}

/// A binding table under construction.
///
/// Scopes are opened and closed as the builder walks, and bindings are
/// registered into whichever scope is current. Keeping the arenas private
/// behind this type is what stops a caller from fabricating a `BindingId` that
/// does not index anything.
#[derive(Debug)]
pub struct BindingTableBuilder {
    scopes: Vec<Scope>,
    drafts: Vec<BindingDraft>,
    resolution: HashMap<NodeKey, BindingId>,
}

impl BindingTableBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope {
                parent: None,
                opener: None,
            }],
            drafts: Vec::new(),
            resolution: HashMap::new(),
        }
    }

    /// Opens a child scope of `parent` and returns its handle.
    pub fn open_scope(&mut self, parent: ScopeId, opener: ByteSpan) -> ScopeId {
        let id = ScopeId::new(self.scopes.len() as u32);
        self.scopes.push(Scope {
            parent: Some(parent),
            opener: Some(opener),
        });
        id
    }

    /// Registers a binding and returns its handle.
    ///
    /// The draft stays open: references and assignments are discovered later
    /// in the walk, so a binding is only sealed when the whole file is done.
    pub fn push_binding(&mut self, draft: BindingDraft) -> BindingId {
        let id = BindingId::new(self.drafts.len() as u32);
        self.drafts.push(draft);
        id
    }

    /// The still-open draft for `id`, for recording a reference or assignment
    /// discovered after the binding was registered.
    pub fn draft_mut(&mut self, id: BindingId) -> &mut BindingDraft {
        &mut self.drafts[id.index()]
    }

    /// Records that the reference at `key` resolves to `binding`.
    ///
    /// The first resolution wins: a reference is resolved by the innermost
    /// binding that can see it, and the builder visits scopes innermost-first.
    pub fn resolve_to(&mut self, key: NodeKey, binding: BindingId) {
        self.resolution.entry(key).or_insert(binding);
    }

    pub fn finish(self) -> BindingTable {
        BindingTable {
            scopes: self.scopes,
            bindings: self.drafts.into_iter().map(BindingDraft::finish).collect(),
            resolution: self.resolution,
        }
    }
}

impl Default for BindingTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::binding::model::{BindingDraft, BindingKind};
    use paredit_core_syntax::sexpr::{ByteOffset, SymbolName};

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    fn table() -> (BindingTable, ScopeId, ScopeId, BindingId) {
        let mut builder = BindingTableBuilder::new();
        let outer = builder.open_scope(ScopeId::FILE, span(0, 40));
        let inner = builder.open_scope(outer, span(10, 30));
        let binding = builder.push_binding(BindingDraft::new(
            SymbolName::new("x").expect("symbol"),
            BindingKind::Variable,
            inner,
            span(12, 13),
        ));
        builder.resolve_to(NodeKey::atom(span(20, 21)), binding);
        (builder.finish(), outer, inner, binding)
    }

    #[test]
    fn nesting_is_reflexive_and_transitive_up_to_the_file_scope() {
        let (table, outer, inner, _) = table();
        assert!(table.is_within(inner, inner));
        assert!(table.is_within(inner, outer));
        assert!(table.is_within(inner, ScopeId::FILE));
        assert!(!table.is_within(outer, inner));
    }

    #[test]
    fn a_recorded_reference_resolves_and_an_unrecorded_one_does_not() {
        let (table, _, _, binding) = table();
        assert_eq!(table.resolve(NodeKey::atom(span(20, 21))), Some(binding));
        assert_eq!(table.resolve(NodeKey::atom(span(99, 100))), None);
    }

    #[test]
    fn the_innermost_resolution_wins() {
        let mut builder = BindingTableBuilder::new();
        let inner = builder.push_binding(BindingDraft::new(
            SymbolName::new("x").expect("symbol"),
            BindingKind::Variable,
            ScopeId::FILE,
            span(0, 1),
        ));
        let outer = builder.push_binding(BindingDraft::new(
            SymbolName::new("x").expect("symbol"),
            BindingKind::Variable,
            ScopeId::FILE,
            span(5, 6),
        ));
        let reference = NodeKey::atom(span(20, 21));
        builder.resolve_to(reference, inner);
        builder.resolve_to(reference, outer);
        assert_eq!(builder.finish().resolve(reference), Some(inner));
    }
}
