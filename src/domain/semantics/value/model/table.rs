//! What each binding and each same-file constant is worth.

use std::collections::HashMap;

use crate::domain::sexpr::SymbolName;

use crate::domain::semantics::binding::BindingId;

use super::literal::PropagatableValue;

/// The constant value of every binding that provably has one, plus the
/// file's `defconstant` definitions.
///
/// Lookup is by [`BindingId`], never by name: two `x` bindings in sibling
/// scopes are different things, and only the binding table knows which one a
/// given occurrence means. The name-keyed half is reserved for file-level
/// constants, which have no enclosing binding to resolve against.
///
/// Absence means "not provably constant". A binding that is reassigned, is
/// special, sits in a scope containing an opaque region, or has a
/// non-constant initial form is simply not here.
#[derive(Debug, Clone, Default)]
pub struct ValueTable {
    bindings: HashMap<BindingId, PropagatableValue>,
    constants: HashMap<SymbolName, PropagatableValue>,
}

impl ValueTable {
    /// The value of a resolved binding, if it provably has a constant one.
    pub fn binding_value(&self, binding: BindingId) -> Option<&PropagatableValue> {
        self.bindings.get(&binding)
    }

    /// The value of a file-level `defconstant`, if the file defines exactly
    /// one with that name.
    pub fn constant_value(&self, name: &SymbolName) -> Option<&PropagatableValue> {
        self.constants.get(name)
    }

    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    pub fn constant_count(&self) -> usize {
        self.constants.len()
    }
}

/// A value table under construction.
#[derive(Debug, Default)]
pub struct ValueTableBuilder {
    bindings: HashMap<BindingId, PropagatableValue>,
    constants: HashMap<SymbolName, PropagatableValue>,
    /// Names seen more than once at file level, which are therefore not
    /// usable as constants regardless of what they were defined to.
    ambiguous_constants: Vec<SymbolName>,
}

impl ValueTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_binding_value(&mut self, binding: BindingId, value: PropagatableValue) {
        self.bindings.insert(binding, value);
    }

    /// Records a file-level constant.
    ///
    /// A second definition of the same name retracts the first rather than
    /// overwriting it: with two `defconstant`s in one file there is no way to
    /// know which one a reference sees, so the honest answer is neither. This
    /// mirrors the conservative rejection in
    /// [`crate::domain::inline_literal_constant`].
    pub fn define_constant(&mut self, name: SymbolName, value: PropagatableValue) {
        if self.ambiguous_constants.contains(&name) {
            return;
        }
        if self.constants.remove(&name).is_some() {
            self.ambiguous_constants.push(name);
            return;
        }
        self.constants.insert(name, value);
    }

    /// Retracts a name defined at file level with a value that is not
    /// provably constant, so a later duplicate cannot make it look known.
    pub fn poison_constant(&mut self, name: SymbolName) {
        self.constants.remove(&name);
        if !self.ambiguous_constants.contains(&name) {
            self.ambiguous_constants.push(name);
        }
    }

    /// The table as it stands, for evaluating the next initial form against.
    ///
    /// Propagation is a fixpoint — a `let*` initial form may reference a
    /// binding resolved in an earlier round — and each round needs to read
    /// what previous rounds proved.
    pub fn snapshot(&self) -> ValueTable {
        ValueTable {
            bindings: self.bindings.clone(),
            constants: self.constants.clone(),
        }
    }

    pub fn finish(self) -> ValueTable {
        ValueTable {
            bindings: self.bindings,
            constants: self.constants,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> SymbolName {
        SymbolName::new(text).expect("symbol")
    }

    #[test]
    fn a_single_definition_is_usable() {
        let mut builder = ValueTableBuilder::new();
        builder.define_constant(name("limit"), PropagatableValue::Integer(10));
        let table = builder.finish();
        assert_eq!(
            table.constant_value(&name("limit")),
            Some(&PropagatableValue::Integer(10))
        );
    }

    #[test]
    fn a_duplicate_definition_retracts_the_name_entirely() {
        let mut builder = ValueTableBuilder::new();
        builder.define_constant(name("limit"), PropagatableValue::Integer(10));
        builder.define_constant(name("limit"), PropagatableValue::Integer(20));
        assert_eq!(builder.finish().constant_value(&name("limit")), None);
    }

    #[test]
    fn a_third_definition_does_not_resurrect_a_retracted_name() {
        let mut builder = ValueTableBuilder::new();
        for value in [10, 20, 30] {
            builder.define_constant(name("limit"), PropagatableValue::Integer(value));
        }
        assert_eq!(builder.finish().constant_value(&name("limit")), None);
    }

    #[test]
    fn a_poisoned_name_stays_unusable() {
        let mut builder = ValueTableBuilder::new();
        builder.poison_constant(name("limit"));
        builder.define_constant(name("limit"), PropagatableValue::Integer(10));
        assert_eq!(builder.finish().constant_value(&name("limit")), None);
    }

    #[test]
    fn an_unrecorded_binding_has_no_value() {
        let table = ValueTableBuilder::new().finish();
        assert_eq!(table.binding_count(), 0);
        assert_eq!(table.constant_count(), 0);
    }
}
