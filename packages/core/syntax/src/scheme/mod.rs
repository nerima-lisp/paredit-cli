//! Scheme and Racket syntax helpers shared across semantic passes.
//!
//! The Common Lisp counterpart of this module is the reason it exists. Until
//! now every dialect fell through `CommonLispOperator::from_head`, which meant
//! Scheme was analysed correctly exactly where the two languages happen to
//! agree -- `let`, `let*`, `lambda` -- and silently not at all everywhere else.
//! `letrec`, `let-values`, `do`, `case-lambda`, `guard` and `parameterize` all
//! bind, and none of them are Common Lisp forms.

mod definition;
mod formals;
mod forms;
mod operator;
mod symbol;

pub use definition::{SchemeDefineTarget, scheme_define_target};
pub use formals::{
    SchemeFormal, SchemeFormalKind, scheme_formal_defaults, scheme_formal_defaults_in,
    scheme_formals, scheme_formals_are_readable, scheme_formals_in,
};
pub use forms::{
    SchemeBindingForm, SchemeBindingNamespace, SchemeDefinitionForm, SchemeImportModifier,
    SchemeLetKind, SchemeLibraryDeclaration,
};
pub use operator::{SchemeOperator, scheme_head_has_registered_semantics};
pub use symbol::{is_scheme_identifier, is_scheme_identifier_text, scheme_identifier_text};

#[cfg(test)]
mod tests;
