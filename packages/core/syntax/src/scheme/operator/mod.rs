mod classify;
mod kind;
mod table;

pub use kind::SchemeOperator;

use crate::definition::DefinitionCategory;

use super::forms::{SchemeBindingForm, SchemeDefinitionForm, SchemeLibraryDeclaration};

impl SchemeOperator {
    /// Resolves an operator head, or `None` for anything this crate has no
    /// structural knowledge of.
    #[must_use]
    pub fn from_head(head: &str) -> Option<Self> {
        table::scheme_operator_from_head(head)
    }

    /// The lexical scope this operator opens, if any.
    #[must_use]
    pub const fn binding_form(self) -> Option<SchemeBindingForm> {
        classify::binding_form(self)
    }

    /// The definition this operator introduces, if any.
    #[must_use]
    pub const fn definition_form(self) -> Option<SchemeDefinitionForm> {
        classify::definition_form(self)
    }

    /// The definition category reported for this operator before the form's
    /// own shape is consulted.
    #[must_use]
    pub const fn definition_category(self) -> Option<DefinitionCategory> {
        classify::definition_category(self)
    }

    /// The `define-library` declaration this operator spells, if any.
    #[must_use]
    pub const fn library_declaration(self) -> Option<SchemeLibraryDeclaration> {
        classify::library_declaration(self)
    }

    /// Whether every argument is ordinary evaluated code.
    #[must_use]
    pub const fn has_transparent_body(self) -> bool {
        classify::has_transparent_body(self)
    }

    /// Whether this operator introduces names the surrounding walk must track.
    #[must_use]
    pub const fn is_binder(self) -> bool {
        self.binding_form().is_some()
    }
}

/// Whether a head names a Scheme form whose semantics this crate knows.
///
/// Used to decide whether a call is safe to look through. An unknown head may
/// be a macro that binds or assigns, so it is not.
#[must_use]
pub fn scheme_head_has_registered_semantics(head: &str) -> bool {
    SchemeOperator::from_head(head).is_some()
}
