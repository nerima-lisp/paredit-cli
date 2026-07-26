//! Arena handles into the binding table.

/// A binding's position in [`super::BindingTable`]'s arena.
///
/// Handles rather than references: the value and type layers hang off the
/// binding table and must be able to name a binding without borrowing it. A
/// `ValueTable` holding `&Binding` would make the two tables one
/// self-referential struct, which is why every cross-table link in the
/// semantic layers is an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(u32);

impl BindingId {
    pub(in crate::domain::semantics) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A lexical scope's position in [`super::BindingTable`]'s scope tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u32);

impl ScopeId {
    /// The file-level scope, which every table has and which has no parent.
    pub const FILE: Self = Self(0);

    pub(in crate::domain::semantics) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_scope_is_the_arena_root() {
        assert_eq!(ScopeId::FILE.index(), 0);
    }

    #[test]
    fn handles_round_trip_through_their_index() {
        assert_eq!(BindingId::new(7).index(), 7);
        assert_eq!(ScopeId::new(3).index(), 3);
    }
}
