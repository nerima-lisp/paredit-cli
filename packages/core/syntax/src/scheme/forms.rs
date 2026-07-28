//! The shapes a Scheme special form can take, named so that the traversal
//! layers can switch on meaning rather than on spelling.
//!
//! These mirror `common_lisp::forms` in spirit: one enum per structural
//! question a caller actually asks, rather than one predicate per operator.

/// How a binding list's initializers see the names the list introduces.
///
/// This is the single axis that separates `let`, `let*` and `letrec`, so they
/// share one handler parameterised by this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeLetKind {
    /// `let`: every initializer is evaluated in the enclosing scope.
    Parallel,
    /// `let*`: each initializer sees the names bound before it.
    Sequential,
    /// `letrec` and `letrec*`: every initializer sees every name in the group,
    /// which is what lets a group of mutually recursive lambdas refer to each
    /// other. R7RS distinguishes the two by evaluation *order*, not by
    /// visibility, and order is not something this layer records.
    Recursive,
}

/// Which namespace a `let`-family form binds into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeBindingNamespace {
    /// Ordinary value bindings.
    Value,
    /// `let-syntax` and `letrec-syntax`: the names are macros, and their
    /// initializers are transformer specs rather than expressions.
    Syntax,
}

/// A Scheme form that opens a lexical scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeBindingForm {
    /// `(let ((name value) ...) body ...)` and its `let*`/`letrec` siblings,
    /// plus the `let-syntax` pair.
    Let {
        /// Initializer visibility.
        kind: SchemeLetKind,
        /// Value or syntax namespace.
        namespace: SchemeBindingNamespace,
    },
    /// `(let name ((var init) ...) body ...)`.
    ///
    /// Only reachable when child 1 is a symbol; the plain `let` shape is
    /// otherwise identical, so the discrimination is left to the caller that
    /// can see the actual form.
    NamedLet,
    /// `(let-values (((formals) producer) ...) body ...)`.
    ///
    /// Distinct from [`Self::Let`] because the bound position is a whole
    /// formals list, not a single name.
    LetValues(SchemeLetKind),
    /// `(do ((var init step) ...) (test result ...) body ...)`.
    Do,
    /// `(lambda formals body ...)`.
    Lambda,
    /// `(case-lambda (formals body ...) ...)`: independently scoped clauses.
    CaseLambda,
    /// `(guard (var clause ...) body ...)`.
    ///
    /// The condition variable is bound over the *clauses* only. The guarded
    /// body runs before the variable exists, so it must be walked in the
    /// enclosing scope.
    Guard,
    /// `(parameterize ((param value) ...) body ...)` and `fluid-let`.
    ///
    /// Binds nothing lexically. Both halves of each pair are ordinary
    /// expressions evaluated in the enclosing scope -- the left-hand side is a
    /// *reference* to an existing parameter or variable, not a new name. Every
    /// other form in this enum introduces names; this one is here precisely so
    /// that the traversal does not mistake its pairs for a binding list.
    DynamicBinding,
}

/// A Scheme form that introduces a top-level or internal definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeDefinitionForm {
    /// `(define name value)`, `(define (name . formals) body ...)`, and the
    /// curried `(define ((name a) b) body ...)`.
    Define,
    /// `(define-values formals producer)`.
    DefineValues,
    /// `(define-record-type name (ctor field ...) pred (field accessor ...) ...)`.
    DefineRecordType,
    /// `(define-syntax name transformer)`.
    DefineSyntax,
    /// Racket's `(define-syntax-rule (name . pattern) template)`.
    DefineSyntaxRule,
    /// `(define-library (name ...) declaration ...)`.
    DefineLibrary,
    /// Racket's `(struct name (field ...) option ...)`.
    Struct,
    /// Racket's `(define-struct name (field ...))`.
    DefineStruct,
    /// Racket's `(define/contract name contract body ...)`.
    DefineContract,
}

impl SchemeDefinitionForm {
    /// Whether the form binds its name in the syntax rather than value
    /// namespace.
    #[must_use]
    pub const fn is_syntax_definition(self) -> bool {
        matches!(self, Self::DefineSyntax | Self::DefineSyntaxRule)
    }
}

/// A declaration that may appear directly inside `define-library`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeLibraryDeclaration {
    /// `(export spec ...)`, where a spec is a name or `(rename from to)`.
    Export,
    /// `(import set ...)`.
    Import,
    /// `(begin body ...)`.
    Begin,
    /// `(include path ...)`.
    Include,
    /// `(include-ci path ...)`.
    IncludeCi,
    /// `(include-library-declarations path ...)`.
    IncludeLibraryDeclarations,
    /// `(cond-expand clause ...)`.
    CondExpand,
}

/// How an import set narrows or renames the library it wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeImportModifier {
    /// `(only set name ...)`.
    Only,
    /// `(except set name ...)`.
    Except,
    /// `(prefix set identifier)`.
    Prefix,
    /// `(rename set (from to) ...)`.
    Rename,
}

impl SchemeImportModifier {
    /// Returns the stable CLI and JSON identifier.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Only => "only",
            Self::Except => "except",
            Self::Prefix => "prefix",
            Self::Rename => "rename",
        }
    }

    /// Resolves an import-set head.
    #[must_use]
    pub fn from_head(head: &str) -> Option<Self> {
        Some(match head {
            "only" => Self::Only,
            "except" => Self::Except,
            "prefix" => Self::Prefix,
            "rename" => Self::Rename,
            _ => return None,
        })
    }
}
