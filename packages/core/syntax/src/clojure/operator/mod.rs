mod classify;
mod kind;
mod table;

pub use kind::{ClojureBindingForm, ClojureOperator, ClojureThreadingForm};
pub use table::normalize_clojure_operator_head;

impl ClojureOperator {
    /// Resolves a list head to a known Clojure operator.
    ///
    /// Returns `None` for any head this tool has no structural knowledge of,
    /// including user-defined macros.
    #[must_use]
    pub fn from_head(head: &str) -> Option<Self> {
        table::clojure_operator_from_head(head)
    }

    /// Returns the definition category this operator introduces, if any.
    #[must_use]
    pub const fn definition_category(self) -> Option<crate::definition::DefinitionCategory> {
        classify::definition_category(self)
    }

    /// Returns whether this operator introduces a named top-level definition.
    #[must_use]
    pub const fn is_definition(self) -> bool {
        self.definition_category().is_some()
    }

    /// Returns whether this operator defines a var private to its namespace.
    ///
    /// `defn-` is the only core form with a dedicated private spelling; `def`
    /// and `defn` express privacy through `^:private` metadata instead, which
    /// is a property of the form rather than of the head.
    #[must_use]
    pub const fn is_private_definition(self) -> bool {
        matches!(self, Self::DefnPrivate)
    }

    /// Returns whether this operator defines a callable with a parameter
    /// vector.
    #[must_use]
    pub const fn is_callable_definition(self) -> bool {
        matches!(
            self,
            Self::Defn | Self::DefnPrivate | Self::Defmacro | Self::Defmethod
        )
    }

    /// Returns whether a parameter-list refactor applies to this operator.
    ///
    /// This matches [`Self::is_callable_definition`]; the two are separate
    /// because locating the parameter vector is what actually constrains the
    /// refactor, and that lives in the definition layer. `defmethod` in
    /// particular hides its parameters behind a dispatch value that may itself
    /// be a vector.
    #[must_use]
    pub const fn supports_parameter_refactor(self) -> bool {
        self.is_callable_definition()
    }

    /// Returns whether this operator defines a macro expander, whose body runs
    /// at compile time and therefore must not be treated as ordinary code.
    #[must_use]
    pub const fn is_macro_expander_definition(self) -> bool {
        matches!(self, Self::Defmacro)
    }

    /// Returns whether a call to a function defined by this operator can be
    /// inlined at its call sites.
    #[must_use]
    pub const fn is_inline_function_definition(self) -> bool {
        matches!(self, Self::Defn | Self::DefnPrivate)
    }

    /// Returns whether this operator declares a namespace.
    #[must_use]
    pub const fn is_namespace_declaration(self) -> bool {
        matches!(self, Self::Ns | Self::InNs)
    }

    /// Returns whether this operator opens a type or protocol body whose
    /// children are method implementations rather than ordinary body forms.
    #[must_use]
    pub const fn has_method_body(self) -> bool {
        matches!(
            self,
            Self::Defrecord
                | Self::Deftype
                | Self::Defprotocol
                | Self::Definterface
                | Self::Reify
                | Self::ExtendType
                | Self::ExtendProtocol
                | Self::Proxy
        )
    }

    /// Returns the lexical binding layout this operator introduces, if any.
    #[must_use]
    pub const fn binding_form(self) -> Option<ClojureBindingForm> {
        classify::binding_form(self)
    }

    /// Returns the threading discipline of this operator, if it is a threading
    /// macro.
    #[must_use]
    pub const fn threading_form(self) -> Option<ClojureThreadingForm> {
        classify::threading_form(self)
    }
}

/// Returns whether a head names a Clojure definition form.
#[must_use]
pub fn is_clojure_definition_head(head: &str) -> bool {
    ClojureOperator::from_head(head).is_some_and(ClojureOperator::is_definition)
}
