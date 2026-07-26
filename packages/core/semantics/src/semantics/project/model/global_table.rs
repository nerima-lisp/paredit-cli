//! Top-level definitions across a project.

use std::collections::HashMap;

use crate::domain::semantics::value::PropagatableValue;

use super::qualified_symbol::QualifiedSymbol;

/// What kind of top-level definition a name has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalKind {
    Function,
    Macro,
    /// `defvar`/`defparameter` — a special variable, whose value can change.
    Variable,
    /// `defconstant` — the only kind whose value may cross a file boundary.
    Constant,
}

/// One top-level definition.
#[derive(Debug, Clone)]
pub struct GlobalDefinition {
    symbol: QualifiedSymbol,
    kind: GlobalKind,
    /// The file it was defined in, as an index into the project's analysis
    /// order.
    file: usize,
}

impl GlobalDefinition {
    pub const fn new(symbol: QualifiedSymbol, kind: GlobalKind, file: usize) -> Self {
        Self { symbol, kind, file }
    }

    pub const fn symbol(&self) -> &QualifiedSymbol {
        &self.symbol
    }

    pub const fn kind(&self) -> GlobalKind {
        self.kind
    }

    pub const fn file(&self) -> usize {
        self.file
    }
}

/// Every top-level definition in a project, keyed by package-qualified name.
///
/// Values cross files only for `defconstant`, and only when exactly one
/// definition exists project-wide. A `defvar` can be rebound at run time and a
/// second `defconstant` makes the value ambiguous, so both are recorded as
/// definitions but carry no value.
#[derive(Debug, Clone, Default)]
pub struct GlobalTable {
    definitions: HashMap<QualifiedSymbol, Vec<GlobalDefinition>>,
    constants: HashMap<QualifiedSymbol, PropagatableValue>,
}

impl GlobalTable {
    /// Every definition of `symbol`. More than one means the project defines
    /// it twice, which callers must treat as ambiguous rather than pick from.
    pub fn definitions(&self, symbol: &QualifiedSymbol) -> &[GlobalDefinition] {
        self.definitions.get(symbol).map_or(&[], Vec::as_slice)
    }

    /// The value of a project-wide constant, when exactly one file defines it
    /// and that definition is provably constant.
    pub fn constant_value(&self, symbol: &QualifiedSymbol) -> Option<&PropagatableValue> {
        self.constants.get(symbol)
    }

    pub fn definition_count(&self) -> usize {
        self.definitions.values().map(Vec::len).sum()
    }

    /// Every project-wide constant, for a caller that needs to find the ones
    /// visible to a file rather than ask about a name it already has.
    ///
    /// A file's own value table is keyed by bare name, so filling it from here
    /// means walking these and keeping the ones whose package the file is in —
    /// there is no other direction to ask the question from.
    pub fn constants(&self) -> impl Iterator<Item = (&QualifiedSymbol, &PropagatableValue)> {
        self.constants.iter()
    }
}

/// A global table under construction.
#[derive(Debug, Default)]
pub struct GlobalTableBuilder {
    definitions: HashMap<QualifiedSymbol, Vec<GlobalDefinition>>,
    constants: HashMap<QualifiedSymbol, PropagatableValue>,
}

impl GlobalTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define(&mut self, definition: GlobalDefinition) {
        self.definitions
            .entry(definition.symbol().clone())
            .or_default()
            .push(definition);
    }

    /// Records a constant's value, retracting it if a second definition of the
    /// same symbol appears anywhere in the project.
    pub fn define_constant(&mut self, symbol: QualifiedSymbol, value: PropagatableValue) {
        if self.constants.contains_key(&symbol) {
            self.constants.remove(&symbol);
            return;
        }
        self.constants.insert(symbol, value);
    }

    pub fn finish(mut self) -> GlobalTable {
        // A symbol defined more than once has no single value, whichever of
        // the definitions were constant.
        self.constants.retain(|symbol, _| {
            self.definitions
                .get(symbol)
                .is_none_or(|all| all.len() == 1)
        });
        GlobalTable {
            definitions: self.definitions,
            constants: self.constants,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::semantics::project::model::PackageId;
    use crate::domain::sexpr::SymbolName;

    fn symbol(package: &str, name: &str) -> QualifiedSymbol {
        QualifiedSymbol::new(
            PackageId::new(package),
            SymbolName::new(name).expect("symbol"),
        )
    }

    #[test]
    fn the_same_name_in_two_packages_stays_two_definitions() {
        let mut builder = GlobalTableBuilder::new();
        builder.define(GlobalDefinition::new(
            symbol("app", "run"),
            GlobalKind::Function,
            0,
        ));
        builder.define(GlobalDefinition::new(
            symbol("test", "run"),
            GlobalKind::Function,
            1,
        ));
        let table = builder.finish();
        assert_eq!(table.definitions(&symbol("app", "run")).len(), 1);
        assert_eq!(table.definitions(&symbol("test", "run")).len(), 1);
    }

    #[test]
    fn a_uniquely_defined_constant_carries_its_value_across_files() {
        let mut builder = GlobalTableBuilder::new();
        builder.define(GlobalDefinition::new(
            symbol("app", "limit"),
            GlobalKind::Constant,
            0,
        ));
        builder.define_constant(symbol("app", "limit"), PropagatableValue::Integer(10));
        assert_eq!(
            builder.finish().constant_value(&symbol("app", "limit")),
            Some(&PropagatableValue::Integer(10))
        );
    }

    #[test]
    fn a_constant_defined_twice_carries_no_value() {
        let mut builder = GlobalTableBuilder::new();
        for file in 0..2 {
            builder.define(GlobalDefinition::new(
                symbol("app", "limit"),
                GlobalKind::Constant,
                file,
            ));
            builder.define_constant(symbol("app", "limit"), PropagatableValue::Integer(10));
        }
        let table = builder.finish();
        assert_eq!(table.definitions(&symbol("app", "limit")).len(), 2);
        assert_eq!(table.constant_value(&symbol("app", "limit")), None);
    }

    #[test]
    fn an_undefined_symbol_has_nothing() {
        let table = GlobalTableBuilder::new().finish();
        assert!(table.definitions(&symbol("app", "run")).is_empty());
        assert_eq!(table.definition_count(), 0);
    }
}
