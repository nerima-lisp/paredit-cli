use std::{fmt, marker::PhantomData};

use crate::domain::common_lisp::{common_lisp_operator_head_eq, common_lisp_symbol_identity_eq};
use crate::domain::definition::DefinitionCategory;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::Dialect;

/// A refactoring operation whose semantic safety must be verified per dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticOperation {
    /// Introduces a dialect-appropriate lexical binding form.
    IntroduceLet,
    /// Renames a lexical binding and its references.
    RenameBinding,
    /// Extracts selected forms into a new function.
    ExtractFunction,
}

impl SemanticOperation {
    /// Returns the stable CLI-facing operation name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::IntroduceLet => "introduce-let",
            Self::RenameBinding => "rename-binding",
            Self::ExtractFunction => "extract-function",
        }
    }
}

mod sealed {
    use super::SemanticOperation;

    pub(crate) trait SemanticOperationMarker {
        const OPERATION: SemanticOperation;
    }
}

/// Type marker for an introduce-let semantic proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntroduceLetOperation;

impl sealed::SemanticOperationMarker for IntroduceLetOperation {
    const OPERATION: SemanticOperation = SemanticOperation::IntroduceLet;
}

/// Type marker for a rename-binding semantic proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenameBindingOperation;

impl sealed::SemanticOperationMarker for RenameBindingOperation {
    const OPERATION: SemanticOperation = SemanticOperation::RenameBinding;
}

/// Type marker for an extract-function semantic proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtractFunctionOperation;

impl sealed::SemanticOperationMarker for ExtractFunctionOperation {
    const OPERATION: SemanticOperation = SemanticOperation::ExtractFunction;
}

/// A path from a semantic form to one of its direct or nested children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelativeNodePath {
    /// A direct child of the form.
    Child(usize),
    /// A child of one of the form's direct children.
    Grandchild {
        /// The direct child index.
        child: usize,
        /// The nested child index.
        grandchild: usize,
    },
}

impl RelativeNodePath {
    /// Returns the first child index in the path.
    #[must_use]
    pub const fn child(self) -> usize {
        match self {
            Self::Child(child) | Self::Grandchild { child, .. } => child,
        }
    }

    /// Returns the nested child index when this is a two-level path.
    #[must_use]
    pub const fn grandchild(self) -> Option<usize> {
        match self {
            Self::Child(_) => None,
            Self::Grandchild { grandchild, .. } => Some(grandchild),
        }
    }
}

/// Describes the parameter list of a callable form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParameterShape {
    container: RelativeNodePath,
    first_parameter_index: usize,
}

impl ParameterShape {
    const fn new(container: RelativeNodePath, first_parameter_index: usize) -> Self {
        Self {
            container,
            first_parameter_index,
        }
    }

    /// Returns the path to the parameter container.
    #[must_use]
    pub const fn container(self) -> RelativeNodePath {
        self.container
    }

    /// Returns the first child in the container that denotes a parameter.
    #[must_use]
    pub const fn first_parameter_index(self) -> usize {
        self.first_parameter_index
    }
}

/// Describes where the executable body of a semantic form begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyShape {
    /// All direct children from this index are body forms.
    ChildrenFrom(usize),
    /// Body forms begin immediately after the node at this path.
    ChildrenAfter(RelativeNodePath),
    /// Each callable clause has body forms beginning at the given child index.
    ClauseChildrenFrom {
        /// Index of the first direct child that is an arity clause.
        first_clause_index: usize,
        /// Index of the first body form inside each arity clause.
        body_child_index: usize,
    },
}

/// A dialect-neutral definition layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionShape {
    category: DefinitionCategory,
    name: Option<RelativeNodePath>,
    parameters: Option<ParameterShape>,
    body: BodyShape,
}

impl DefinitionShape {
    const fn new(
        category: DefinitionCategory,
        name: Option<RelativeNodePath>,
        parameters: Option<ParameterShape>,
        body: BodyShape,
    ) -> Self {
        Self {
            category,
            name,
            parameters,
            body,
        }
    }

    /// Returns the semantic category of this definition.
    #[must_use]
    pub const fn category(self) -> DefinitionCategory {
        self.category
    }

    /// Returns the definition name path, if the form has a name.
    #[must_use]
    pub const fn name(self) -> Option<RelativeNodePath> {
        self.name
    }

    /// Returns the callable parameter layout, if present.
    #[must_use]
    pub const fn parameters(self) -> Option<ParameterShape> {
        self.parameters
    }

    /// Returns the body layout.
    #[must_use]
    pub const fn body(self) -> BodyShape {
        self.body
    }
}

/// Determines whether binding initializers can see earlier bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingVisibility {
    /// Every initializer is evaluated in the enclosing scope.
    Parallel,
    /// Each initializer can reference preceding bindings.
    Sequential,
}

/// Describes where a scope obtains its lexical binders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinderShape {
    /// A container whose children are binding entries such as `(name value)`.
    BindingList {
        /// Path to the binding-entry container.
        container: RelativeNodePath,
        /// Path from each binding entry to its name.
        name: RelativeNodePath,
        /// Path from each binding entry to its initializer.
        initializer: Option<RelativeNodePath>,
        /// Visibility of earlier bindings from later initializers.
        visibility: BindingVisibility,
    },
    /// A named scope plus a container of binding entries, as in Scheme named let.
    NamedBindingList {
        /// Path to the name bound over the scope body.
        scope_name: RelativeNodePath,
        /// Path to the binding-entry container.
        container: RelativeNodePath,
        /// Path from each binding entry to its name.
        name: RelativeNodePath,
        /// Path from each binding entry to its initializer.
        initializer: Option<RelativeNodePath>,
        /// Visibility of earlier bindings from later initializers.
        visibility: BindingVisibility,
    },
    /// Alternating name and initializer nodes in one flat container.
    FlatPairs {
        /// Path to the flat binding container.
        container: RelativeNodePath,
        /// Index of the first binding name.
        first_name_index: usize,
        /// Number of children occupied by each binding pair.
        stride: usize,
        /// Visibility of earlier bindings from later initializers.
        visibility: BindingVisibility,
    },
    /// A callable parameter list.
    Parameters(ParameterShape),
    /// A callable name and parameter list that are both bound over its body.
    NamedParameters {
        /// Path to the callable's local name.
        name: RelativeNodePath,
        /// Parameter layout relative to the callable form.
        parameters: ParameterShape,
    },
    /// Parameter lists repeated in independently scoped callable clauses.
    ParameterClauses {
        /// Optional path to a callable name bound over every clause body.
        name: Option<RelativeNodePath>,
        /// Index of the first direct child that is an arity clause.
        first_clause_index: usize,
        /// Parameter layout relative to each arity clause.
        parameters: ParameterShape,
    },
}

/// A dialect-neutral lexical scope layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeShape {
    binders: BinderShape,
    body: BodyShape,
}

impl ScopeShape {
    const fn new(binders: BinderShape, body: BodyShape) -> Self {
        Self { binders, body }
    }

    /// Returns the lexical binder layout.
    #[must_use]
    pub const fn binders(self) -> BinderShape {
        self.binders
    }

    /// Returns the executable body layout.
    #[must_use]
    pub const fn body(self) -> BodyShape {
        self.body
    }
}

/// Semantic metadata and verification rules used inside the domain layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DialectSemanticPolicy {
    dialect: Dialect,
}

impl DialectSemanticPolicy {
    pub(crate) const fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }

    pub(crate) const fn dialect(self) -> Dialect {
        self.dialect
    }

    pub(crate) const fn supports(self, operation: SemanticOperation) -> bool {
        matches!(
            (self.dialect, operation),
            (
                Dialect::CommonLisp
                    | Dialect::EmacsLisp
                    | Dialect::Lfe
                    | Dialect::Scheme
                    | Dialect::Racket
                    | Dialect::Clojure
                    | Dialect::Hy
                    | Dialect::Carp
                    | Dialect::Janet
                    | Dialect::Fennel,
                SemanticOperation::IntroduceLet
                    | SemanticOperation::RenameBinding
                    | SemanticOperation::ExtractFunction,
            )
        )
    }

    fn verify<O: sealed::SemanticOperationMarker>(
        self,
    ) -> Result<VerifiedSemanticPolicy<O>, UnsupportedSemanticOperation> {
        if self.supports(O::OPERATION) {
            Ok(VerifiedSemanticPolicy {
                policy: self,
                operation: PhantomData,
            })
        } else {
            Err(UnsupportedSemanticOperation {
                dialect: self.dialect,
                operation: O::OPERATION,
            })
        }
    }

    pub(crate) fn identifiers_equal(self, candidate: &str, expected: &str) -> bool {
        match self.dialect {
            Dialect::CommonLisp => common_lisp_symbol_identity_eq(candidate, expected),
            Dialect::EmacsLisp
            | Dialect::Lfe
            | Dialect::Scheme
            | Dialect::Racket
            | Dialect::Clojure
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Janet
            | Dialect::Fennel
            | Dialect::Unknown => candidate == expected,
        }
    }

    pub(crate) fn definition_shape(self, form: &ExpressionView) -> Option<DefinitionShape> {
        definition_shape(self, form)
    }

    pub(crate) fn scope_shape(self, form: &ExpressionView) -> Option<ScopeShape> {
        scope_shape(self, form)
    }
}

impl Dialect {
    /// Verifies that introduce-let has semantic support for this dialect.
    pub fn verify_introduce_let(
        self,
    ) -> Result<VerifiedSemanticPolicy<IntroduceLetOperation>, UnsupportedSemanticOperation> {
        DialectSemanticPolicy::new(self).verify()
    }

    /// Verifies that rename-binding has semantic support for this dialect.
    pub fn verify_rename_binding(
        self,
    ) -> Result<VerifiedSemanticPolicy<RenameBindingOperation>, UnsupportedSemanticOperation> {
        DialectSemanticPolicy::new(self).verify()
    }

    /// Verifies that extract-function has semantic support for this dialect.
    pub fn verify_extract_function(
        self,
    ) -> Result<VerifiedSemanticPolicy<ExtractFunctionOperation>, UnsupportedSemanticOperation>
    {
        DialectSemanticPolicy::new(self).verify()
    }
}

/// Proof that semantic operation `O` is verified for a dialect.
///
/// The operation marker is part of the token type, so a proof for one
/// operation cannot be passed to an API requiring another operation.
/// Raw policy construction is intentionally unavailable outside the crate.
///
/// ```compile_fail
/// use paredit_cli::dialect::DialectSemanticPolicy;
/// ```
///
/// ```compile_fail
/// use paredit_cli::dialect::{
///     IntroduceLetOperation, RenameBindingOperation, VerifiedSemanticPolicy,
/// };
///
/// fn requires_rename(_: Option<VerifiedSemanticPolicy<RenameBindingOperation>>) {}
/// let introduce: Option<VerifiedSemanticPolicy<IntroduceLetOperation>> = None;
/// requires_rename(introduce);
/// ```
///
/// Its private fields also prevent safe callers from forging a proof.
///
/// ```compile_fail
/// use paredit_cli::dialect::{RenameBindingOperation, VerifiedSemanticPolicy};
///
/// let _forged = VerifiedSemanticPolicy::<RenameBindingOperation> {};
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedSemanticPolicy<O> {
    policy: DialectSemanticPolicy,
    operation: PhantomData<fn() -> O>,
}

impl<O> VerifiedSemanticPolicy<O> {
    /// Returns the verified dialect.
    #[must_use]
    pub const fn dialect(self) -> Dialect {
        self.policy.dialect()
    }

    /// Compares identifiers using the verified dialect's identity rules.
    #[must_use]
    pub fn identifiers_equal(self, candidate: &str, expected: &str) -> bool {
        self.policy.identifiers_equal(candidate, expected)
    }

    /// Resolves a definition layout after validating the actual form.
    #[must_use]
    pub fn definition_shape(self, form: &ExpressionView) -> Option<DefinitionShape> {
        self.policy.definition_shape(form)
    }

    /// Resolves a lexical scope layout after validating the actual form.
    #[must_use]
    pub fn scope_shape(self, form: &ExpressionView) -> Option<ScopeShape> {
        self.policy.scope_shape(form)
    }
}

impl VerifiedSemanticPolicy<IntroduceLetOperation> {
    /// Returns the operation verified by this token type.
    #[must_use]
    pub const fn operation(self) -> SemanticOperation {
        SemanticOperation::IntroduceLet
    }
}

impl VerifiedSemanticPolicy<RenameBindingOperation> {
    /// Returns the operation verified by this token type.
    #[must_use]
    pub const fn operation(self) -> SemanticOperation {
        SemanticOperation::RenameBinding
    }
}

impl VerifiedSemanticPolicy<ExtractFunctionOperation> {
    /// Returns the operation verified by this token type.
    #[must_use]
    pub const fn operation(self) -> SemanticOperation {
        SemanticOperation::ExtractFunction
    }
}

/// Failure to verify a semantic operation for a dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedSemanticOperation {
    dialect: Dialect,
    operation: SemanticOperation,
}

impl UnsupportedSemanticOperation {
    /// Returns the unsupported dialect.
    #[must_use]
    pub const fn dialect(self) -> Dialect {
        self.dialect
    }

    /// Returns the unverified operation.
    #[must_use]
    pub const fn operation(self) -> SemanticOperation {
        self.operation
    }
}

impl fmt::Display for UnsupportedSemanticOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "semantic operation {} is not verified for {:?}",
            self.operation.label(),
            self.dialect
        )
    }
}

impl std::error::Error for UnsupportedSemanticOperation {}

const DIRECT_FUNCTION: DefinitionShape = DefinitionShape::new(
    DefinitionCategory::Function,
    Some(RelativeNodePath::Child(1)),
    Some(ParameterShape::new(RelativeNodePath::Child(2), 0)),
    BodyShape::ChildrenFrom(3),
);
const DIRECT_MACRO: DefinitionShape = DefinitionShape::new(
    DefinitionCategory::Macro,
    Some(RelativeNodePath::Child(1)),
    Some(ParameterShape::new(RelativeNodePath::Child(2), 0)),
    BodyShape::ChildrenFrom(3),
);
const DIRECT_VARIABLE: DefinitionShape = DefinitionShape::new(
    DefinitionCategory::Variable,
    Some(RelativeNodePath::Child(1)),
    None,
    BodyShape::ChildrenFrom(2),
);
const SCHEME_FUNCTION_DEFINE: DefinitionShape = DefinitionShape::new(
    DefinitionCategory::Function,
    Some(RelativeNodePath::Grandchild {
        child: 1,
        grandchild: 0,
    }),
    Some(ParameterShape::new(RelativeNodePath::Child(1), 1)),
    BodyShape::ChildrenFrom(2),
);
const SCHEME_SYNTAX_DEFINE: DefinitionShape = DefinitionShape::new(
    DefinitionCategory::Macro,
    Some(RelativeNodePath::Child(1)),
    None,
    BodyShape::ChildrenFrom(2),
);

fn definition_shape(
    policy: DialectSemanticPolicy,
    form: &ExpressionView,
) -> Option<DefinitionShape> {
    let head = form_head(form)?;

    match policy.dialect {
        Dialect::CommonLisp if common_lisp_operator_head_eq(head, "defun") => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_FUNCTION)
        }
        Dialect::CommonLisp if common_lisp_operator_head_eq(head, "defmacro") => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_MACRO)
        }
        Dialect::CommonLisp
            if common_lisp_operator_head_eq(head, "defvar")
                || common_lisp_operator_head_eq(head, "defparameter") =>
        {
            direct_variable_shape(form)
        }
        Dialect::EmacsLisp if head == "defun" => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_FUNCTION)
        }
        Dialect::EmacsLisp if head == "defmacro" => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_MACRO)
        }
        Dialect::EmacsLisp if matches!(head, "defvar" | "defconst" | "defcustom") => {
            direct_variable_shape(form)
        }
        Dialect::Lfe if head == "defun" => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_FUNCTION)
        }
        Dialect::Lfe if head == "defmacro" => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_MACRO)
        }
        Dialect::Scheme | Dialect::Racket if head == "define" => scheme_define_shape(form),
        Dialect::Scheme | Dialect::Racket if head == "define-syntax" => {
            scheme_define_syntax_shape(form)
        }
        Dialect::Clojure | Dialect::Hy if head == "defn" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Clojure | Dialect::Hy if head == "defmacro" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Clojure if head == "def" => direct_variable_shape(form),
        Dialect::Hy if head == "setv" => direct_variable_shape(form),
        Dialect::Carp if head == "defn" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Carp if head == "def" => direct_variable_shape(form),
        Dialect::Janet if matches!(head, "defn" | "defn-") => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Janet if head == "defmacro" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Janet if matches!(head, "def" | "def-") => direct_variable_shape(form),
        Dialect::Fennel if head == "fn" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Fennel if head == "macro" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Fennel if matches!(head, "local" | "global") => direct_variable_shape(form),
        Dialect::Unknown
        | Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Clojure
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Janet
        | Dialect::Fennel => None,
    }
}

fn direct_callable_shape(
    form: &ExpressionView,
    parameter_delimiter: Delimiter,
    shape: DefinitionShape,
) -> Option<DefinitionShape> {
    (form.children.len() >= 3
        && atom_text(form.children.get(1)?).is_some()
        && is_plain_list(form.children.get(2)?, parameter_delimiter))
    .then_some(shape)
}

fn direct_variable_shape(form: &ExpressionView) -> Option<DefinitionShape> {
    (form.children.len() >= 2 && atom_text(form.children.get(1)?).is_some())
        .then_some(DIRECT_VARIABLE)
}

fn scheme_define_shape(form: &ExpressionView) -> Option<DefinitionShape> {
    if form.children.len() < 3 {
        return None;
    }

    let target = form.children.get(1)?;
    if atom_text(target).is_some() {
        return Some(DIRECT_VARIABLE);
    }

    (is_plain_list(target, Delimiter::Paren)
        && target.children.first().and_then(atom_text).is_some())
    .then_some(SCHEME_FUNCTION_DEFINE)
}

fn scheme_define_syntax_shape(form: &ExpressionView) -> Option<DefinitionShape> {
    (form.children.len() == 3 && atom_text(form.children.get(1)?).is_some())
        .then_some(SCHEME_SYNTAX_DEFINE)
}

const LIST_BINDINGS_PARALLEL: BinderShape = BinderShape::BindingList {
    container: RelativeNodePath::Child(1),
    name: RelativeNodePath::Child(0),
    initializer: Some(RelativeNodePath::Child(1)),
    visibility: BindingVisibility::Parallel,
};
const LIST_BINDINGS_SEQUENTIAL: BinderShape = BinderShape::BindingList {
    container: RelativeNodePath::Child(1),
    name: RelativeNodePath::Child(0),
    initializer: Some(RelativeNodePath::Child(1)),
    visibility: BindingVisibility::Sequential,
};
const FLAT_BINDINGS_SEQUENTIAL: BinderShape = BinderShape::FlatPairs {
    container: RelativeNodePath::Child(1),
    first_name_index: 0,
    stride: 2,
    visibility: BindingVisibility::Sequential,
};
const PARAMETER_SCOPE: ScopeShape = ScopeShape::new(
    BinderShape::Parameters(ParameterShape::new(RelativeNodePath::Child(1), 0)),
    BodyShape::ChildrenFrom(2),
);
const LIST_LET_SCOPE: ScopeShape =
    ScopeShape::new(LIST_BINDINGS_PARALLEL, BodyShape::ChildrenFrom(2));
const LIST_LET_STAR_SCOPE: ScopeShape =
    ScopeShape::new(LIST_BINDINGS_SEQUENTIAL, BodyShape::ChildrenFrom(2));
const FLAT_LET_SCOPE: ScopeShape =
    ScopeShape::new(FLAT_BINDINGS_SEQUENTIAL, BodyShape::ChildrenFrom(2));
const SCHEME_NAMED_LET_SCOPE: ScopeShape = ScopeShape::new(
    BinderShape::NamedBindingList {
        scope_name: RelativeNodePath::Child(1),
        container: RelativeNodePath::Child(2),
        name: RelativeNodePath::Child(0),
        initializer: Some(RelativeNodePath::Child(1)),
        visibility: BindingVisibility::Parallel,
    },
    BodyShape::ChildrenFrom(3),
);

fn scope_shape(policy: DialectSemanticPolicy, form: &ExpressionView) -> Option<ScopeShape> {
    let head = form_head(form)?;

    match policy.dialect {
        Dialect::CommonLisp if common_lisp_operator_head_eq(head, "let") => {
            list_scope(form, Delimiter::Paren, LIST_LET_SCOPE)
        }
        Dialect::CommonLisp if common_lisp_operator_head_eq(head, "let*") => {
            list_scope(form, Delimiter::Paren, LIST_LET_STAR_SCOPE)
        }
        Dialect::CommonLisp if common_lisp_operator_head_eq(head, "lambda") => {
            parameter_scope(form, Delimiter::Paren, PARAMETER_SCOPE)
        }
        Dialect::EmacsLisp if head == "let" => list_scope(form, Delimiter::Paren, LIST_LET_SCOPE),
        Dialect::EmacsLisp if head == "let*" => {
            list_scope(form, Delimiter::Paren, LIST_LET_STAR_SCOPE)
        }
        Dialect::EmacsLisp if head == "lambda" => {
            parameter_scope(form, Delimiter::Paren, PARAMETER_SCOPE)
        }
        Dialect::Lfe if head == "let" => list_scope(form, Delimiter::Paren, LIST_LET_SCOPE),
        Dialect::Lfe if head == "let*" => list_scope(form, Delimiter::Paren, LIST_LET_STAR_SCOPE),
        Dialect::Lfe if head == "lambda" => {
            parameter_scope(form, Delimiter::Paren, PARAMETER_SCOPE)
        }
        Dialect::Scheme | Dialect::Racket if head == "let" => scheme_let_scope(form),
        Dialect::Scheme | Dialect::Racket if head == "let*" => {
            list_scope(form, Delimiter::Paren, LIST_LET_STAR_SCOPE)
        }
        Dialect::Scheme | Dialect::Racket if head == "lambda" => {
            parameter_scope(form, Delimiter::Paren, PARAMETER_SCOPE)
        }
        Dialect::Clojure if head == "let" => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Clojure if head == "fn" => clojure_fn_scope(form),
        Dialect::Hy if head == "let" => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Hy if head == "fn" => parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE),
        Dialect::Carp if head == "let" => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Carp if head == "fn" => parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE),
        Dialect::Janet if head == "let" => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Janet if head == "fn" => {
            parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE)
        }
        Dialect::Fennel if head == "let" => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Fennel if head == "fn" => {
            parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE)
        }
        Dialect::Unknown
        | Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Clojure
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Janet
        | Dialect::Fennel => None,
    }
}

fn list_scope(
    form: &ExpressionView,
    binding_delimiter: Delimiter,
    shape: ScopeShape,
) -> Option<ScopeShape> {
    (form.children.len() >= 3 && is_plain_list(form.children.get(1)?, binding_delimiter))
        .then_some(shape)
}

fn flat_scope(form: &ExpressionView, shape: ScopeShape) -> Option<ScopeShape> {
    let bindings = form.children.get(1)?;
    (form.children.len() >= 3
        && is_plain_list(bindings, Delimiter::Bracket)
        && bindings.children.len() % 2 == 0)
        .then_some(shape)
}

fn parameter_scope(
    form: &ExpressionView,
    parameter_delimiter: Delimiter,
    shape: ScopeShape,
) -> Option<ScopeShape> {
    (form.children.len() >= 3 && is_plain_list(form.children.get(1)?, parameter_delimiter))
        .then_some(shape)
}

fn scheme_let_scope(form: &ExpressionView) -> Option<ScopeShape> {
    if form.children.len() >= 3
        && form
            .children
            .get(1)
            .is_some_and(|bindings| is_plain_list(bindings, Delimiter::Paren))
    {
        return Some(LIST_LET_SCOPE);
    }

    (form.children.len() >= 4
        && form.children.get(1).and_then(atom_text).is_some()
        && form
            .children
            .get(2)
            .is_some_and(|bindings| is_plain_list(bindings, Delimiter::Paren)))
    .then_some(SCHEME_NAMED_LET_SCOPE)
}

fn clojure_fn_scope(form: &ExpressionView) -> Option<ScopeShape> {
    let first = form.children.get(1)?;
    let (name, first_shape_index) = if atom_text(first).is_some() {
        (Some(RelativeNodePath::Child(1)), 2)
    } else {
        (None, 1)
    };
    let first_shape = form.children.get(first_shape_index)?;

    if is_plain_list(first_shape, Delimiter::Bracket) {
        if form.children.len() <= first_shape_index + 1 {
            return None;
        }

        let parameters = ParameterShape::new(RelativeNodePath::Child(first_shape_index), 0);
        let binders = name.map_or(BinderShape::Parameters(parameters), |name| {
            BinderShape::NamedParameters { name, parameters }
        });
        return Some(ScopeShape::new(
            binders,
            BodyShape::ChildrenFrom(first_shape_index + 1),
        ));
    }

    let clauses = &form.children[first_shape_index..];
    if clauses.is_empty() || !clauses.iter().all(valid_clojure_arity_clause) {
        return None;
    }

    Some(ScopeShape::new(
        BinderShape::ParameterClauses {
            name,
            first_clause_index: first_shape_index,
            parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
        },
        BodyShape::ClauseChildrenFrom {
            first_clause_index: first_shape_index,
            body_child_index: 1,
        },
    ))
}

fn valid_clojure_arity_clause(clause: &ExpressionView) -> bool {
    is_plain_list(clause, Delimiter::Paren)
        && clause.children.len() >= 2
        && clause
            .children
            .first()
            .is_some_and(|parameters| is_plain_list(parameters, Delimiter::Bracket))
}

fn form_head(form: &ExpressionView) -> Option<&str> {
    if !is_plain_list(form, Delimiter::Paren) {
        return None;
    }
    form.children.first().and_then(atom_text)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then_some(view.text.as_deref())
        .flatten()
}

fn is_plain_list(view: &ExpressionView, delimiter: Delimiter) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(delimiter)
        && view.reader_prefixes.is_empty()
}

#[cfg(test)]
mod tests {
    use crate::domain::sexpr::SyntaxTree;

    use super::*;

    const OPERATIONS: [SemanticOperation; 3] = [
        SemanticOperation::IntroduceLet,
        SemanticOperation::RenameBinding,
        SemanticOperation::ExtractFunction,
    ];

    fn parsed_form(source: &str, dialect: Dialect) -> ExpressionView {
        let root = SyntaxTree::parse_with_dialect(source, dialect)
            .expect("fixture parses")
            .root_view();

        root.children
            .first()
            .cloned()
            .expect("fixture has one form")
    }

    fn verified_dialect(
        dialect: Dialect,
        operation: SemanticOperation,
    ) -> Result<Dialect, UnsupportedSemanticOperation> {
        match operation {
            SemanticOperation::IntroduceLet => dialect
                .verify_introduce_let()
                .map(VerifiedSemanticPolicy::dialect),
            SemanticOperation::RenameBinding => dialect
                .verify_rename_binding()
                .map(VerifiedSemanticPolicy::dialect),
            SemanticOperation::ExtractFunction => dialect
                .verify_extract_function()
                .map(VerifiedSemanticPolicy::dialect),
        }
    }

    #[test]
    fn semantic_support_matrix_covers_all_eighteen_dialect_operation_cells() {
        let cases = [
            (Dialect::CommonLisp, true),
            (Dialect::EmacsLisp, true),
            (Dialect::Scheme, true),
            (Dialect::Clojure, true),
            (Dialect::Janet, true),
            (Dialect::Fennel, true),
        ];
        let mut checked_cells = 0;

        for (dialect, supported) in cases {
            let policy = DialectSemanticPolicy::new(dialect);
            for operation in OPERATIONS {
                assert_eq!(policy.supports(operation), supported, "{dialect:?}");
                assert_eq!(
                    verified_dialect(dialect, operation).ok(),
                    supported.then_some(dialect),
                    "{dialect:?}: {operation:?}"
                );
                checked_cells += 1;
            }
        }

        assert_eq!(checked_cells, 18);
    }

    #[test]
    fn unknown_dialect_fails_closed_for_every_verification_entry() {
        let policy = DialectSemanticPolicy::new(Dialect::Unknown);

        for operation in OPERATIONS {
            assert!(!policy.supports(operation));
            let error = verified_dialect(Dialect::Unknown, operation)
                .expect_err("Unknown must fail every operation-specific factory");
            assert_eq!(error.dialect(), Dialect::Unknown);
            assert_eq!(error.operation(), operation);
        }
    }

    #[test]
    fn verified_token_type_is_bound_to_its_operation() {
        fn accepts_rename(_: VerifiedSemanticPolicy<RenameBindingOperation>) {}

        let verified = Dialect::CommonLisp
            .verify_rename_binding()
            .expect("Common Lisp rename-binding is verified");

        accepts_rename(verified);
        assert_eq!(verified.dialect(), Dialect::CommonLisp);
        assert_eq!(verified.operation(), SemanticOperation::RenameBinding);
    }

    #[test]
    fn common_lisp_identifier_equality_is_package_aware_and_conservative() {
        let policy = DialectSemanticPolicy::new(Dialect::CommonLisp);

        assert!(policy.identifiers_equal(":X", ":x"));
        assert!(policy.identifiers_equal("A:X", "a::x"));
        assert!(policy.identifiers_equal("CL:X", "COMMON-LISP:x"));
        assert!(policy.identifiers_equal("A:|X|", "a:x"));

        assert!(!policy.identifiers_equal("A:X", "B:X"));
        assert!(!policy.identifiers_equal("A:X", "X"));
        assert!(!policy.identifiers_equal("X", "A:X"));
        assert!(!policy.identifiers_equal("#:X", "X"));
        assert!(!policy.identifiers_equal("#:X", "#:X"));
        assert!(!policy.identifiers_equal("#:X", "#:x"));
        assert!(!policy.identifiers_equal("A:|x|", "A:X"));
        assert!(!policy.identifiers_equal("|a|:X", "A:X"));
    }

    #[test]
    fn non_common_lisp_identifier_equality_is_exact() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let policy = DialectSemanticPolicy::new(dialect);
            assert!(policy.identifiers_equal("same", "same"), "{dialect:?}");
            assert!(!policy.identifiers_equal("Widget", "widget"), "{dialect:?}");
        }
    }

    #[test]
    fn definition_shape_matrix_covers_all_six_dialects() {
        let cases = [
            (
                Dialect::CommonLisp,
                "(defun f (x) x)",
                DefinitionCategory::Function,
            ),
            (
                Dialect::EmacsLisp,
                "(defun f (x) x)",
                DefinitionCategory::Function,
            ),
            (
                Dialect::Scheme,
                "(define (f x) x)",
                DefinitionCategory::Function,
            ),
            (
                Dialect::Clojure,
                "(defn f [x] x)",
                DefinitionCategory::Function,
            ),
            (
                Dialect::Janet,
                "(defn f [x] x)",
                DefinitionCategory::Function,
            ),
            (
                Dialect::Fennel,
                "(macro m [x] x)",
                DefinitionCategory::Macro,
            ),
        ];

        for (dialect, source, category) in cases {
            let form = parsed_form(source, dialect);
            let shape = DialectSemanticPolicy::new(dialect)
                .definition_shape(&form)
                .expect("known definition form");
            assert_eq!(shape.category(), category, "{dialect:?}");
        }
    }

    #[test]
    fn scheme_definition_resolver_discriminates_actual_form_shape() {
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let variable = parsed_form("(define answer 42)", Dialect::Scheme);
        let function = parsed_form("(define (answer x) x)", Dialect::Scheme);
        let syntax = parsed_form("(define-syntax when transformer)", Dialect::Scheme);

        assert_eq!(
            policy
                .definition_shape(&variable)
                .map(DefinitionShape::category),
            Some(DefinitionCategory::Variable)
        );
        let function_shape = policy
            .definition_shape(&function)
            .expect("function define shape");
        assert_eq!(function_shape.category(), DefinitionCategory::Function);
        assert_eq!(
            function_shape.name(),
            Some(RelativeNodePath::Grandchild {
                child: 1,
                grandchild: 0,
            })
        );
        assert_eq!(
            function_shape.parameters(),
            Some(ParameterShape::new(RelativeNodePath::Child(1), 1))
        );

        let syntax_shape = policy
            .definition_shape(&syntax)
            .expect("define-syntax shape");
        assert_eq!(syntax_shape.category(), DefinitionCategory::Macro);
        assert_eq!(syntax_shape.name(), Some(RelativeNodePath::Child(1)));
        assert_eq!(syntax_shape.parameters(), None);
        assert_eq!(syntax_shape.body(), BodyShape::ChildrenFrom(2));
    }

    #[test]
    fn definition_resolver_rejects_unverified_shapes() {
        let cases = [
            (Dialect::Scheme, "(define)"),
            (Dialect::Scheme, "(define (f))"),
            (Dialect::Scheme, "(define-syntax x)"),
            (Dialect::Scheme, "(define-syntax (x) transformer)"),
            (Dialect::Clojure, "(defn f (not-a-parameter-vector) body)"),
            (Dialect::Unknown, "(defun f (x) x)"),
        ];

        for (dialect, source) in cases {
            let form = parsed_form(source, dialect);
            assert_eq!(
                DialectSemanticPolicy::new(dialect).definition_shape(&form),
                None,
                "{dialect:?}: {source}"
            );
        }
    }

    #[test]
    fn scope_shape_matrix_covers_all_six_dialects() {
        let cases = [
            (
                Dialect::CommonLisp,
                "(let ((x 1)) x)",
                LIST_BINDINGS_PARALLEL,
            ),
            (
                Dialect::EmacsLisp,
                "(let ((x 1)) x)",
                LIST_BINDINGS_PARALLEL,
            ),
            (Dialect::Scheme, "(let ((x 1)) x)", LIST_BINDINGS_PARALLEL),
            (Dialect::Clojure, "(let [x 1] x)", FLAT_BINDINGS_SEQUENTIAL),
            (Dialect::Janet, "(let [x 1] x)", FLAT_BINDINGS_SEQUENTIAL),
            (Dialect::Fennel, "(let [x 1] x)", FLAT_BINDINGS_SEQUENTIAL),
        ];

        for (dialect, source, binders) in cases {
            let form = parsed_form(source, dialect);
            let shape = DialectSemanticPolicy::new(dialect)
                .scope_shape(&form)
                .expect("known let scope");
            assert_eq!(shape.binders(), binders, "{dialect:?}");
            assert_eq!(shape.body(), BodyShape::ChildrenFrom(2), "{dialect:?}");
        }
    }

    #[test]
    fn scheme_named_let_uses_shifted_binding_and_body_paths() {
        let form = parsed_form("(let loop ((x 1)) (loop x))", Dialect::Scheme);
        let shape = DialectSemanticPolicy::new(Dialect::Scheme)
            .scope_shape(&form)
            .expect("named let scope");

        assert_eq!(shape, SCHEME_NAMED_LET_SCOPE);
        assert_eq!(shape.body(), BodyShape::ChildrenFrom(3));

        let malformed = parsed_form("(let loop body)", Dialect::Scheme);
        assert_eq!(
            DialectSemanticPolicy::new(Dialect::Scheme).scope_shape(&malformed),
            None
        );
    }

    #[test]
    fn clojure_fn_resolver_handles_optional_name_and_multi_arity() {
        let policy = DialectSemanticPolicy::new(Dialect::Clojure);
        let anonymous = parsed_form("(fn [x] x)", Dialect::Clojure);
        let named = parsed_form("(fn add [x] x)", Dialect::Clojure);
        let multi = parsed_form("(fn ([x] x) ([x y] y))", Dialect::Clojure);
        let named_multi = parsed_form("(fn add ([x] x) ([x y] y))", Dialect::Clojure);

        assert_eq!(policy.scope_shape(&anonymous), Some(PARAMETER_SCOPE));
        assert_eq!(
            policy.scope_shape(&named),
            Some(ScopeShape::new(
                BinderShape::NamedParameters {
                    name: RelativeNodePath::Child(1),
                    parameters: ParameterShape::new(RelativeNodePath::Child(2), 0),
                },
                BodyShape::ChildrenFrom(3),
            ))
        );
        assert_eq!(
            policy.scope_shape(&multi),
            Some(clojure_multi_arity_scope(None, 1))
        );
        assert_eq!(
            policy.scope_shape(&named_multi),
            Some(clojure_multi_arity_scope(
                Some(RelativeNodePath::Child(1)),
                2,
            ))
        );
    }

    #[test]
    fn clojure_fn_resolver_fails_closed_on_unverified_shapes() {
        let policy = DialectSemanticPolicy::new(Dialect::Clojure);

        for source in [
            "(fn add)",
            "(fn [x])",
            "(fn ([x]))",
            "(fn add (x))",
            "(fn ([x] x) malformed)",
        ] {
            let form = parsed_form(source, Dialect::Clojure);
            assert_eq!(policy.scope_shape(&form), None, "{source}");
        }
    }

    fn clojure_multi_arity_scope(
        name: Option<RelativeNodePath>,
        first_clause_index: usize,
    ) -> ScopeShape {
        ScopeShape::new(
            BinderShape::ParameterClauses {
                name,
                first_clause_index,
                parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
            },
            BodyShape::ClauseChildrenFrom {
                first_clause_index,
                body_child_index: 1,
            },
        )
    }

    #[test]
    fn unknown_dialect_has_no_semantic_shapes() {
        let policy = DialectSemanticPolicy::new(Dialect::Unknown);
        let definition = parsed_form("(defun f (x) x)", Dialect::Unknown);
        let scope = parsed_form("(let ((x 1)) x)", Dialect::Unknown);

        assert_eq!(policy.definition_shape(&definition), None);
        assert_eq!(policy.scope_shape(&scope), None);
    }
}
