use std::{fmt, marker::PhantomData};

use crate::common_lisp::{common_lisp_operator_head_eq, common_lisp_symbol_identity_eq};
use crate::definition::DefinitionCategory;
use crate::emacs_lisp::EmacsLispOperator;
use crate::scheme::{
    SchemeBindingForm, SchemeDefineTarget, SchemeDefinitionForm, SchemeLetKind, SchemeOperator,
    scheme_define_target,
};
use crate::sexpr::{Delimiter, ExpressionKind, ExpressionView};

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

    pub trait SemanticOperationMarker {
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
    /// Every initializer can reference every binding in the group, including
    /// its own and those written after it.
    ///
    /// Scheme's `letrec`/`letrec*`, and what makes a group of mutually
    /// recursive procedures work. Treating it as [`Self::Sequential`] would
    /// leave the initializers *before* the shadowing entry looking unshadowed.
    Recursive,
}

/// How many children of a bare-name container are actually binding names.
///
/// Iteration forms such as Fennel's `each` put the names first and the value
/// that drives the loop last, so the count is not always the container length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameListArity {
    /// Exactly this many names; every later child drives the form.
    Exact(usize),
    /// Every child is a name except this many trailing driver expressions.
    AllButLast(usize),
}

impl NameListArity {
    /// Returns how many of `available` children are binding names, or `None`
    /// when the container is too short to satisfy the arity.
    #[must_use]
    pub const fn name_count(self, available: usize) -> Option<usize> {
        match self {
            Self::Exact(count) => {
                if available >= count {
                    Some(count)
                } else {
                    None
                }
            }
            Self::AllButLast(drivers) => available.checked_sub(drivers),
        }
    }
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
    /// Bare binding names in a container, with no per-name initializer.
    ///
    /// Fennel's iteration forms use this: `(each [k v (pairs t)] ...)` binds
    /// `k` and `v`, and the trailing child is the iterator rather than a name.
    NameList {
        /// Path to the container holding the names.
        container: RelativeNodePath,
        /// Index of the first child in the container that is a name.
        first_name_index: usize,
        /// How many children starting at `first_name_index` are names.
        names: NameListArity,
    },
    /// A single bare binding name at a fixed path, as in Janet's `each`.
    SingleName {
        /// Path to the bound name.
        name: RelativeNodePath,
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
pub struct DialectSemanticPolicy {
    dialect: Dialect,
}

impl DialectSemanticPolicy {
    pub const fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }

    pub const fn dialect(self) -> Dialect {
        self.dialect
    }

    pub const fn supports(self, operation: SemanticOperation) -> bool {
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

    pub fn identifiers_equal(self, candidate: &str, expected: &str) -> bool {
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

    pub fn definition_shape(self, form: &ExpressionView) -> Option<DefinitionShape> {
        definition_shape(self, form)
    }

    pub fn scope_shape(self, form: &ExpressionView) -> Option<ScopeShape> {
        scope_shape(self, form)
    }
}

impl Dialect {
    /// Reports whether this dialect has verified semantics for `operation`.
    ///
    /// This is the same decision the `verify_*` factories make, exposed so that
    /// callers can describe the support matrix without minting a proof token.
    ///
    /// # Examples
    ///
    /// ```
    /// use paredit_core_syntax::dialect::{Dialect, SemanticOperation};
    ///
    /// assert!(Dialect::Fennel.supports_semantic_operation(SemanticOperation::RenameBinding));
    /// assert!(!Dialect::Unknown.supports_semantic_operation(SemanticOperation::RenameBinding));
    /// ```
    #[must_use]
    pub const fn supports_semantic_operation(self, operation: SemanticOperation) -> bool {
        DialectSemanticPolicy::new(self).supports(operation)
    }

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
/// use paredit_core_syntax::dialect::DialectSemanticPolicy;
/// ```
///
/// ```compile_fail
/// use paredit_core_syntax::dialect::{
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
/// use paredit_core_syntax::dialect::{RenameBindingOperation, VerifiedSemanticPolicy};
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
/// Resolves a definition that names a type-like entity: the head introduces a
/// name but nothing after it is a lexical binder. Carp's `deftype` and Hy's
/// `defclass` are of this shape.
fn named_only(form: &ExpressionView, category: DefinitionCategory) -> Option<DefinitionShape> {
    (form.children.len() >= 2 && atom_text(form.children.get(1)?).is_some()).then(|| {
        DefinitionShape::new(
            category,
            Some(RelativeNodePath::Child(1)),
            None,
            BodyShape::ChildrenFrom(2),
        )
    })
}

/// Resolves a definition whose arities are separate pattern-matching clauses,
/// as in LFE's `(defun f ((x) body) ((x y) body))`.
fn clause_definition_shape(
    form: &ExpressionView,
    category: DefinitionCategory,
) -> Option<DefinitionShape> {
    let clauses = form.children.get(2..)?;
    (form.children.len() >= 3
        && atom_text(form.children.get(1)?).is_some()
        && clauses
            .iter()
            .all(|clause| is_arity_clause(clause, Delimiter::Paren)))
    .then(|| {
        DefinitionShape::new(
            category,
            Some(RelativeNodePath::Child(1)),
            None,
            BodyShape::ClauseChildrenFrom {
                first_clause_index: 2,
                body_child_index: 1,
            },
        )
    })
}
/// `(define-syntax-rule (name pattern ...) template)`: the name sits inside
/// the pattern list, as it does for a procedure `define`.
const SCHEME_SYNTAX_RULE_DEFINE: DefinitionShape = DefinitionShape::new(
    DefinitionCategory::Macro,
    Some(RelativeNodePath::Grandchild {
        child: 1,
        grandchild: 0,
    }),
    Some(ParameterShape::new(RelativeNodePath::Child(1), 1)),
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
        Dialect::EmacsLisp => emacs_lisp_definition_shape(head, form),
        // LFE `defun`/`defmacro` have two layouts: a single parameter list, or
        // one pattern-matching clause per arity. Without the clause case the
        // first clause is read as the parameter list, so `((x) (* x x))` binds
        // both `x` and `(* x x)` as parameters.
        // The clause layout is tried first because its first clause is itself a
        // paren list, so the single-parameter-list reading would accept it and
        // then treat the clause body as a second parameter.
        Dialect::Lfe if head == "defun" => {
            clause_definition_shape(form, DefinitionCategory::Function)
                .or_else(|| direct_callable_shape(form, Delimiter::Paren, DIRECT_FUNCTION))
        }
        Dialect::Lfe if head == "defmacro" => {
            clause_definition_shape(form, DefinitionCategory::Macro)
                .or_else(|| direct_callable_shape(form, Delimiter::Paren, DIRECT_MACRO))
        }
        Dialect::Lfe if head == "defrecord" => named_only(form, DefinitionCategory::Struct),
        Dialect::Lfe if head == "defmodule" => named_only(form, DefinitionCategory::Package),
        Dialect::Scheme | Dialect::Racket => scheme_definition_shape(form, head),
        // `defn-` is `defn` with the var marked private; the layout is
        // identical. `declare` is deliberately absent: it names several vars
        // at once, so no single-name shape describes it.
        Dialect::Clojure if matches!(head, "defn" | "defn-") => {
            clojure_defn_shape(form, DefinitionCategory::Function)
        }
        Dialect::Hy if head == "defn" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Clojure if head == "defmacro" => {
            clojure_defn_shape(form, DefinitionCategory::Macro)
        }
        Dialect::Hy if head == "defmacro" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Clojure if matches!(head, "def" | "defonce") => direct_variable_shape(form),
        Dialect::Hy if matches!(head, "setv" | "setx") => direct_variable_shape(form),
        Dialect::Hy if head == "defclass" => named_only(form, DefinitionCategory::Class),
        // Carp splits its dynamic (compile-time) bindings in two: `defdynamic`
        // names a value, `defndynamic` names a function with a parameter list.
        Dialect::Carp if matches!(head, "defn" | "defndynamic") => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Carp if head == "defmacro" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Carp if matches!(head, "def" | "defdynamic") => direct_variable_shape(form),
        Dialect::Carp if head == "deftype" => named_only(form, DefinitionCategory::Struct),
        Dialect::Carp if head == "defmodule" => named_only(form, DefinitionCategory::Package),
        Dialect::Carp if head == "definterface" => named_only(form, DefinitionCategory::Other),
        Dialect::Janet if matches!(head, "defn" | "defn-" | "varfn") => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Janet if matches!(head, "defmacro" | "defmacro-") => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Janet if matches!(head, "def" | "def-" | "var" | "var-") => {
            direct_variable_shape(form)
        }
        // Fennel's `fn` and `lambda` are the same layout; `lambda` (and its
        // `λ` spelling) only adds runtime arity checks.
        Dialect::Fennel if matches!(head, "fn" | "lambda" | "λ") => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_FUNCTION)
        }
        Dialect::Fennel if head == "macro" => {
            direct_callable_shape(form, Delimiter::Bracket, DIRECT_MACRO)
        }
        Dialect::Fennel if matches!(head, "local" | "global" | "var") => {
            direct_variable_shape(form)
        }
        Dialect::Unknown
        | Dialect::CommonLisp
        | Dialect::Lfe
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

/// Scheme's definition forms, resolved against Scheme's own operator table.
fn scheme_definition_shape(form: &ExpressionView, head: &str) -> Option<DefinitionShape> {
    let definition = SchemeOperator::from_head(head)?.definition_form()?;

    match definition {
        SchemeDefinitionForm::Define | SchemeDefinitionForm::DefineContract => {
            scheme_define_shape(form)
        }
        SchemeDefinitionForm::DefineSyntax => scheme_define_syntax_shape(form),
        SchemeDefinitionForm::DefineSyntaxRule => scheme_define_syntax_rule_shape(form),
        SchemeDefinitionForm::DefineRecordType => {
            scheme_named_definition_shape(form, DefinitionCategory::Struct)
        }
        SchemeDefinitionForm::Struct | SchemeDefinitionForm::DefineStruct => {
            scheme_named_definition_shape(form, DefinitionCategory::Struct)
        }
        // `(define-values (a b) producer)` names more than one variable, and
        // this model has room for exactly one name. Reporting the first would
        // make the others invisible to a rename, so it reports none.
        SchemeDefinitionForm::DefineValues => None,
        // A library name is a *list* -- `(scheme base)` -- not a symbol.
        SchemeDefinitionForm::DefineLibrary => None,
    }
}

fn scheme_define_shape(form: &ExpressionView) -> Option<DefinitionShape> {
    if form.children.len() < 3 {
        return None;
    }

    match scheme_define_target(form.children.get(1)?)? {
        SchemeDefineTarget::Variable { .. } => Some(DIRECT_VARIABLE),
        // A curried `(define ((adder n) x) ...)` puts the name three levels
        // down, one more than `RelativeNodePath` can address. The traversal
        // layer reads it via `scheme_define_target`; here it is declined.
        SchemeDefineTarget::Procedure { formals, .. } if formals.len() == 1 => {
            Some(SCHEME_FUNCTION_DEFINE)
        }
        SchemeDefineTarget::Procedure { .. } => None,
    }
}

fn scheme_define_syntax_shape(form: &ExpressionView) -> Option<DefinitionShape> {
    (form.children.len() == 3 && atom_text(form.children.get(1)?).is_some())
        .then_some(SCHEME_SYNTAX_DEFINE)
}

/// `(define-syntax-rule (name pattern ...) template)`.
fn scheme_define_syntax_rule_shape(form: &ExpressionView) -> Option<DefinitionShape> {
    let pattern = form.children.get(1)?;
    (form.children.len() >= 3
        && scheme_binding_container(pattern)
        && pattern.children.first().and_then(atom_text).is_some())
    .then_some(SCHEME_SYNTAX_RULE_DEFINE)
}

/// A form whose child 1 is the defined name and whose rest is declarations:
/// `define-record-type`, and Racket's `struct` and `define-struct`.
fn scheme_named_definition_shape(
    form: &ExpressionView,
    category: DefinitionCategory,
) -> Option<DefinitionShape> {
    (form.children.len() >= 2 && atom_text(form.children.get(1)?).is_some()).then_some(
        DefinitionShape::new(
            category,
            Some(RelativeNodePath::Child(1)),
            None,
            BodyShape::ChildrenFrom(2),
        ),
    )
}

/// Emacs Lisp definition layouts.
///
/// Resolved through [`EmacsLispOperator`] rather than by comparing head text,
/// so a `.el` file's `DEFUN` — a symbol a user may well have defined — is not
/// read as the special form.
fn emacs_lisp_definition_shape(head: &str, form: &ExpressionView) -> Option<DefinitionShape> {
    match EmacsLispOperator::from_head(head)? {
        // `cl-defun` and `cl-defsubst` take a full Common Lisp lambda list
        // rather than Emacs Lisp's `&optional`/`&rest`-only one, but the
        // *layout* — name, then arglist, then body — is the same, and layout
        // is all this shape records.
        EmacsLispOperator::Defun
        | EmacsLispOperator::Defsubst
        | EmacsLispOperator::ClDefun
        | EmacsLispOperator::ClDefsubst => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_FUNCTION)
        }
        EmacsLispOperator::Defmacro
        | EmacsLispOperator::ClDefmacro
        | EmacsLispOperator::DefineInline => {
            direct_callable_shape(form, Delimiter::Paren, DIRECT_MACRO)
        }
        EmacsLispOperator::Defvar
        | EmacsLispOperator::DefvarLocal
        | EmacsLispOperator::Defconst
        | EmacsLispOperator::Defcustom => direct_variable_shape(form),
        _ => None,
    }
}

/// Emacs Lisp lexical-scope layouts.
///
/// Only the forms whose bindings are plain symbols visible across a single
/// contiguous body are listed. The `if-let*` family is deliberately absent:
/// its ELSE forms are siblings of the THEN form but evaluate with none of the
/// bindings in scope, and [`BodyShape::ChildrenFrom`] cannot say that — a
/// rename driven by this shape would rewrite an occurrence in the ELSE branch
/// that refers to something else entirely. The binding table models those
/// forms directly instead.
fn emacs_lisp_scope_shape(head: &str, form: &ExpressionView) -> Option<ScopeShape> {
    match EmacsLispOperator::from_head(head)? {
        EmacsLispOperator::Let => list_scope(form, Delimiter::Paren, LIST_LET_SCOPE),
        // `dlet` binds dynamically rather than lexically, but it binds the
        // same names in the same places as `let*`, which is what a rename
        // needs to know.
        EmacsLispOperator::LetStar | EmacsLispOperator::Dlet => {
            list_scope(form, Delimiter::Paren, LIST_LET_STAR_SCOPE)
        }
        EmacsLispOperator::Lambda => parameter_scope(form, Delimiter::Paren, PARAMETER_SCOPE),
        _ => None,
    }
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
/// Every child of the binder container is a name, as in Janet's
/// `(with-syms [a b] ...)`.
const BARE_NAMES_SCOPE: ScopeShape = ScopeShape::new(
    BinderShape::NameList {
        container: RelativeNodePath::Child(1),
        first_name_index: 0,
        names: NameListArity::AllButLast(0),
    },
    BodyShape::ChildrenFrom(2),
);
/// Names first, one trailing expression driving the form, as in Fennel's
/// `(each [k v (pairs t)] ...)`.
const ITERATOR_DRIVEN_SCOPE: ScopeShape = ScopeShape::new(
    BinderShape::NameList {
        container: RelativeNodePath::Child(1),
        first_name_index: 0,
        names: NameListArity::AllButLast(1),
    },
    BodyShape::ChildrenFrom(2),
);
/// One name followed by numeric range expressions, as in Fennel's
/// `(for [i 1 10] ...)`.
const RANGE_DRIVEN_SCOPE: ScopeShape = ScopeShape::new(
    BinderShape::NameList {
        container: RelativeNodePath::Child(1),
        first_name_index: 0,
        names: NameListArity::Exact(1),
    },
    BodyShape::ChildrenFrom(2),
);
/// A bare name, the collection it walks, then the body: Janet's `each`.
const SINGLE_NAME_ITERATION_SCOPE: ScopeShape = ScopeShape::new(
    BinderShape::SingleName {
        name: RelativeNodePath::Child(1),
    },
    BodyShape::ChildrenFrom(3),
);
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
        Dialect::EmacsLisp => emacs_lisp_scope_shape(head, form),
        Dialect::Lfe if head == "let" => list_scope(form, Delimiter::Paren, LIST_LET_SCOPE),
        Dialect::Lfe if head == "let*" => list_scope(form, Delimiter::Paren, LIST_LET_STAR_SCOPE),
        Dialect::Lfe if head == "lambda" => {
            parameter_scope(form, Delimiter::Paren, PARAMETER_SCOPE)
        }
        Dialect::Lfe if head == "match-lambda" => clause_scope(form, None, 1, Delimiter::Paren),
        // Each clause of a pattern-matching `defun`/`defmacro` scopes its own
        // parameters, which a single form-relative parameter list cannot say.
        Dialect::Lfe if matches!(head, "defun" | "defmacro") => (form.children.len() >= 3
            && atom_text(form.children.get(1)?).is_some())
        .then(|| clause_scope(form, Some(RelativeNodePath::Child(1)), 2, Delimiter::Paren))
        .flatten(),
        Dialect::Scheme | Dialect::Racket => scheme_scope_shape(form, head),
        Dialect::Clojure if head == "let" => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Clojure if head == "fn" => clojure_fn_scope(form),
        // Every one of these takes a `[name init ...]` vector whose
        // initializers are evaluated in order, exactly like `let`. Clojure has
        // no parallel binding form, so they all share one shape.
        Dialect::Clojure
            if matches!(
                head,
                "loop"
                    | "binding"
                    | "with-open"
                    | "with-redefs"
                    | "with-local-vars"
                    | "if-let"
                    | "if-some"
                    | "when-let"
                    | "when-some"
                    | "when-first"
            ) =>
        {
            flat_scope(form, FLAT_LET_SCOPE)
        }
        Dialect::Clojure if matches!(head, "defn" | "defn-" | "defmacro") => {
            clojure_defn_scope(form)
        }
        // Hy's `for` and `with` take the same bracketed `name value` pairs as
        // its `let`, so all three share the flat-pair layout.
        Dialect::Hy if matches!(head, "let" | "for" | "with") => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Hy if head == "fn" => parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE),
        Dialect::Carp if matches!(head, "let" | "let-do") => flat_scope(form, FLAT_LET_SCOPE),
        Dialect::Carp if head == "fn" => parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE),
        // Janet's resource and conditional binding forms all take one bracketed
        // `name value` pair list. `if-let`/`if-with` are deliberately absent:
        // their else branch is outside the binding's scope, which this shape
        // vocabulary cannot express.
        Dialect::Janet
            if matches!(
                head,
                "let" | "with" | "with-vars" | "when-let" | "when-with"
            ) =>
        {
            flat_scope(form, FLAT_LET_SCOPE)
        }
        Dialect::Janet if head == "with-syms" => bare_name_scope(form),
        Dialect::Janet if head == "each" => single_name_scope(form),
        Dialect::Janet if head == "fn" => janet_fn_scope(form),
        Dialect::Fennel if matches!(head, "let" | "with-open") => flat_scope(form, FLAT_LET_SCOPE),
        // Fennel's comprehensions bind leading names and take the value that
        // drives them as the last child of the same bracket.
        Dialect::Fennel if matches!(head, "each" | "collect" | "icollect" | "accumulate") => {
            iteration_scope(form, ITERATOR_DRIVEN_SCOPE)
        }
        Dialect::Fennel if matches!(head, "for" | "fcollect") => {
            iteration_scope(form, RANGE_DRIVEN_SCOPE)
        }
        Dialect::Fennel if matches!(head, "fn" | "lambda" | "λ") => {
            parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE)
        }
        Dialect::Unknown
        | Dialect::CommonLisp
        | Dialect::Lfe
        | Dialect::Clojure
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Janet
        | Dialect::Fennel => None,
    }
}

/// Scheme's scope-opening forms, resolved against Scheme's own operator table.
///
/// Only the forms whose layout this model can express are reported. `do`,
/// `let-values` and `guard` bind in ways `BinderShape` has no vocabulary for --
/// a step form, a formals list in the name position, a variable scoped to the
/// clauses but not the body -- and returning an approximate shape for them
/// would be worse than returning none: the reference query
/// (`lexical_scope::traversal::binding_forms::scheme`) handles all three
/// exactly, and a `None` here leaves the callers that consume shapes reporting
/// "unsupported" rather than something confidently wrong.
fn scheme_scope_shape(form: &ExpressionView, head: &str) -> Option<ScopeShape> {
    let binding = SchemeOperator::from_head(head)?.binding_form()?;

    match binding {
        SchemeBindingForm::Let { kind, .. } => scheme_let_scope(form, kind),
        SchemeBindingForm::NamedLet => scheme_named_let_scope(form),
        SchemeBindingForm::Lambda => scheme_lambda_scope(form),
        SchemeBindingForm::CaseLambda => scheme_case_lambda_scope(form),
        // `(let-values (((a b) producer) ...) body ...)` fits the ordinary
        // binding-list shape: the bound position is a formals list rather than
        // one name, and the destructuring readers already return every name in
        // a pattern.
        SchemeBindingForm::LetValues(kind) => scheme_list_scope(
            form,
            2,
            ScopeShape::new(
                scheme_binding_list(scheme_visibility(kind)),
                BodyShape::ChildrenFrom(2),
            ),
        ),
        // `do` carries a *step* form per entry -- `(i 0 (+ i 1))` -- and this
        // model has room for one initializer. Reporting the entry as an
        // ordinary binding would drop the step from a rename and silently
        // produce code referring to a name that no longer exists, so `do` is
        // declined here and handled exactly by the reference query.
        SchemeBindingForm::Do
        // `guard` binds its variable over the clauses but not over the guarded
        // body, which no `BodyShape` can express.
        | SchemeBindingForm::Guard
        // `parameterize` opens no lexical scope at all.
        | SchemeBindingForm::DynamicBinding => None,
    }
}

fn scheme_let_scope(form: &ExpressionView, kind: SchemeLetKind) -> Option<ScopeShape> {
    // `(let loop ((x 1)) ...)` shares its head with the unnamed form and is
    // told apart only by child 1 being a symbol.
    if form.children.get(1).and_then(atom_text).is_some() {
        return scheme_named_let_scope(form);
    }

    let shape = ScopeShape::new(
        scheme_binding_list(scheme_visibility(kind)),
        BodyShape::ChildrenFrom(2),
    );
    scheme_list_scope(form, 2, shape)
}

fn scheme_named_let_scope(form: &ExpressionView) -> Option<ScopeShape> {
    if form.children.len() < 4 || form.children.get(1).and_then(atom_text).is_none() {
        return None;
    }
    scheme_binding_container(form.children.get(2)?).then_some(SCHEME_NAMED_LET_SCOPE)
}

fn scheme_lambda_scope(form: &ExpressionView) -> Option<ScopeShape> {
    // `(lambda args body)` binds one rest parameter rather than a list of
    // them; that shape has no `ParameterShape` and is left to the traversal.
    scheme_list_scope(form, 2, PARAMETER_SCOPE)
}

fn scheme_case_lambda_scope(form: &ExpressionView) -> Option<ScopeShape> {
    let clauses = form.children.get(1..)?;
    if clauses.is_empty() || !clauses.iter().all(valid_scheme_lambda_clause) {
        return None;
    }

    Some(ScopeShape::new(
        BinderShape::ParameterClauses {
            name: None,
            first_clause_index: 1,
            parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
        },
        BodyShape::ClauseChildrenFrom {
            first_clause_index: 1,
            body_child_index: 1,
        },
    ))
}

fn valid_scheme_lambda_clause(clause: &ExpressionView) -> bool {
    scheme_binding_container(clause)
        && clause.children.len() >= 2
        && clause
            .children
            .first()
            .is_some_and(scheme_binding_container)
}

const fn scheme_visibility(kind: SchemeLetKind) -> BindingVisibility {
    match kind {
        SchemeLetKind::Parallel => BindingVisibility::Parallel,
        SchemeLetKind::Sequential => BindingVisibility::Sequential,
        SchemeLetKind::Recursive => BindingVisibility::Recursive,
    }
}

const fn scheme_binding_list(visibility: BindingVisibility) -> BinderShape {
    BinderShape::BindingList {
        container: RelativeNodePath::Child(1),
        name: RelativeNodePath::Child(0),
        initializer: Some(RelativeNodePath::Child(1)),
        visibility,
    }
}

fn scheme_list_scope(
    form: &ExpressionView,
    minimum_children: usize,
    shape: ScopeShape,
) -> Option<ScopeShape> {
    (form.children.len() > minimum_children
        && form.children.get(1).is_some_and(scheme_binding_container))
    .then_some(shape)
}

/// Whether a node can hold binding entries or parameters.
///
/// Brackets count: R6RS 4.2.1 makes them interchangeable with parens, and
/// `(let ([x 1]) x)` is the ordinary spelling in Racket.
fn scheme_binding_container(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && matches!(view.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
        && view.reader_prefixes.is_empty()
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

/// Resolves a scope whose binder container holds only bare names.
fn bare_name_scope(form: &ExpressionView) -> Option<ScopeShape> {
    let names = form.children.get(1)?;
    (form.children.len() >= 3
        && is_plain_list(names, Delimiter::Bracket)
        && !names.children.is_empty()
        && names.children.iter().all(|name| atom_text(name).is_some()))
    .then_some(BARE_NAMES_SCOPE)
}

/// Resolves a scope that binds names at the front of its container and takes
/// the remaining children as the expressions driving the iteration.
fn iteration_scope(form: &ExpressionView, shape: ScopeShape) -> Option<ScopeShape> {
    let BinderShape::NameList { names, .. } = shape.binders() else {
        return None;
    };
    let container = form.children.get(1)?;
    if form.children.len() < 3 || !is_plain_list(container, Delimiter::Bracket) {
        return None;
    }
    let name_count = names.name_count(container.children.len())?;
    (name_count > 0
        && container
            .children
            .iter()
            .take(name_count)
            .all(|name| atom_text(name).is_some()))
    .then_some(shape)
}

/// Resolves a scope that binds one bare name ahead of the value it walks.
fn single_name_scope(form: &ExpressionView) -> Option<ScopeShape> {
    (form.children.len() >= 4 && atom_text(form.children.get(1)?).is_some())
        .then_some(SINGLE_NAME_ITERATION_SCOPE)
}

/// Resolves Janet's `fn`, whose name is optional: `(fn [x] x)` and
/// `(fn named [x] x)` are both valid.
fn janet_fn_scope(form: &ExpressionView) -> Option<ScopeShape> {
    let first = form.children.get(1)?;
    if is_plain_list(first, Delimiter::Bracket) {
        return parameter_scope(form, Delimiter::Bracket, PARAMETER_SCOPE);
    }

    (form.children.len() >= 4
        && atom_text(first).is_some()
        && is_plain_list(form.children.get(2)?, Delimiter::Bracket))
    .then_some(ScopeShape::new(
        BinderShape::NamedParameters {
            name: RelativeNodePath::Child(1),
            parameters: ParameterShape::new(RelativeNodePath::Child(2), 0),
        },
        BodyShape::ChildrenFrom(3),
    ))
}

/// Resolves a form whose arities are separate pattern-matching clauses, as in
/// LFE's `match-lambda` and its clause-style `defun`.
fn clause_scope(
    form: &ExpressionView,
    name: Option<RelativeNodePath>,
    first_clause_index: usize,
    parameter_delimiter: Delimiter,
) -> Option<ScopeShape> {
    let clauses = form.children.get(first_clause_index..)?;
    (!clauses.is_empty()
        && clauses
            .iter()
            .all(|clause| is_arity_clause(clause, parameter_delimiter)))
    .then_some(ScopeShape::new(
        BinderShape::ParameterClauses {
            name,
            first_clause_index,
            parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
        },
        BodyShape::ClauseChildrenFrom {
            first_clause_index,
            body_child_index: 1,
        },
    ))
}

/// Where the callable parts of a Clojure `defn` form sit.
///
/// `defn` accepts an optional docstring and an optional attribute map between
/// the name and the parameters, and it accepts either one parameter vector or
/// a run of `([params] body)` arity clauses. All eight combinations are
/// idiomatic, and a docstring in particular is the norm rather than the
/// exception, so a shape that only recognises `(defn name [params] body)`
/// misses most real definitions.
#[derive(Clone, Copy)]
enum ClojureDefnLayout {
    /// One parameter vector at this child index; the body follows it.
    SingleArity { parameters: usize },
    /// A run of arity clauses starting at this child index.
    MultiArity { first_clause: usize },
}

fn clojure_defn_layout(form: &ExpressionView) -> Option<ClojureDefnLayout> {
    // The name must be a plain symbol; a quasiquoted or otherwise computed
    // designator has no statically resolvable layout.
    atom_text(form.children.get(1)?)?;

    // Skip the docstring and attribute map, which are the only things allowed
    // between the name and the parameters.
    let mut index = 2;
    while let Some(child) = form.children.get(index) {
        let is_docstring = atom_text(child).is_some_and(|text| text.starts_with('"'));
        let is_attribute_map = is_plain_list(child, Delimiter::Brace);
        if !is_docstring && !is_attribute_map {
            break;
        }
        index += 1;
    }

    let first = form.children.get(index)?;
    if is_plain_list(first, Delimiter::Bracket) {
        return Some(ClojureDefnLayout::SingleArity { parameters: index });
    }

    let clauses = form.children.get(index..)?;
    (!clauses.is_empty()
        && clauses
            .iter()
            .all(|clause| is_arity_clause(clause, Delimiter::Bracket)))
    .then_some(ClojureDefnLayout::MultiArity {
        first_clause: index,
    })
}

fn clojure_defn_shape(
    form: &ExpressionView,
    category: DefinitionCategory,
) -> Option<DefinitionShape> {
    let name = Some(RelativeNodePath::Child(1));
    Some(match clojure_defn_layout(form)? {
        ClojureDefnLayout::SingleArity { parameters } => DefinitionShape::new(
            category,
            name,
            Some(ParameterShape::new(RelativeNodePath::Child(parameters), 0)),
            BodyShape::ChildrenFrom(parameters + 1),
        ),
        // Each arity clause carries its own parameter list, so no single
        // `ParameterShape` describes the form.
        ClojureDefnLayout::MultiArity { first_clause } => DefinitionShape::new(
            category,
            name,
            None,
            BodyShape::ClauseChildrenFrom {
                first_clause_index: first_clause,
                body_child_index: 1,
            },
        ),
    })
}

/// The lexical scope a `defn` opens over its body.
///
/// Unlike `fn`, the name is a namespace-level var rather than a lexical
/// binding, so it is not part of the binder shape: only the parameters are.
fn clojure_defn_scope(form: &ExpressionView) -> Option<ScopeShape> {
    Some(match clojure_defn_layout(form)? {
        ClojureDefnLayout::SingleArity { parameters } => ScopeShape::new(
            BinderShape::Parameters(ParameterShape::new(RelativeNodePath::Child(parameters), 0)),
            BodyShape::ChildrenFrom(parameters + 1),
        ),
        ClojureDefnLayout::MultiArity { first_clause } => ScopeShape::new(
            BinderShape::ParameterClauses {
                name: None,
                first_clause_index: first_clause,
                parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
            },
            BodyShape::ClauseChildrenFrom {
                first_clause_index: first_clause,
                body_child_index: 1,
            },
        ),
    })
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
    if clauses.is_empty()
        || !clauses
            .iter()
            .all(|clause| is_arity_clause(clause, Delimiter::Bracket))
    {
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

/// Reports whether a form is one arity clause: a parameter list followed by at
/// least one body form. Clojure spells the parameter list with brackets and
/// LFE with parens, so the delimiter is the caller's decision.
fn is_arity_clause(clause: &ExpressionView, parameter_delimiter: Delimiter) -> bool {
    is_plain_list(clause, Delimiter::Paren)
        && clause.children.len() >= 2
        && clause
            .children
            .first()
            .is_some_and(|parameters| is_plain_list(parameters, parameter_delimiter))
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
    use crate::sexpr::SyntaxTree;

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
    fn lfe_clause_defun_scopes_each_clause_instead_of_reading_one_parameter_list() {
        let policy = DialectSemanticPolicy::new(Dialect::Lfe);
        let clauses = parsed_form("(defun f ((x) (* x x)) ((x y) (* x y)))", Dialect::Lfe);

        // Read as a single parameter list, `((x) (* x x))` would bind both `(x)`
        // and `(* x x)` as parameters and swallow the first clause's body.
        assert_eq!(
            policy.scope_shape(&clauses),
            Some(ScopeShape::new(
                BinderShape::ParameterClauses {
                    name: Some(RelativeNodePath::Child(1)),
                    first_clause_index: 2,
                    parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
                },
                BodyShape::ClauseChildrenFrom {
                    first_clause_index: 2,
                    body_child_index: 1,
                },
            ))
        );

        // A single parameter list is still read as one, not as a clause.
        let direct = parsed_form("(defun f (x) (* x x))", Dialect::Lfe);
        assert_eq!(policy.scope_shape(&direct), None);
        assert_eq!(
            policy.definition_shape(&direct).map(DefinitionShape::body),
            Some(BodyShape::ChildrenFrom(3))
        );
    }

    #[test]
    fn fennel_iteration_forms_bind_leading_names_and_leave_the_driver_alone() {
        let policy = DialectSemanticPolicy::new(Dialect::Fennel);
        let cases = [
            ("(each [k v (pairs t)] k)", NameListArity::AllButLast(1)),
            (
                "(icollect [_ v (ipairs t)] v)",
                NameListArity::AllButLast(1),
            ),
            (
                "(accumulate [a 0 _ v (ipairs t)] a)",
                NameListArity::AllButLast(1),
            ),
            ("(for [i 1 10] i)", NameListArity::Exact(1)),
            ("(fcollect [i 1 10] i)", NameListArity::Exact(1)),
        ];

        for (source, names) in cases {
            let form = parsed_form(source, Dialect::Fennel);
            assert_eq!(
                policy.scope_shape(&form).map(ScopeShape::binders),
                Some(BinderShape::NameList {
                    container: RelativeNodePath::Child(1),
                    first_name_index: 0,
                    names,
                }),
                "{source}"
            );
        }
    }

    #[test]
    fn name_list_arity_reports_how_many_children_are_names() {
        assert_eq!(NameListArity::Exact(1).name_count(3), Some(1));
        assert_eq!(NameListArity::Exact(3).name_count(2), None);
        assert_eq!(NameListArity::AllButLast(1).name_count(3), Some(2));
        assert_eq!(NameListArity::AllButLast(1).name_count(0), None);
        assert_eq!(NameListArity::AllButLast(0).name_count(2), Some(2));
    }

    #[test]
    fn shallow_dialect_scopes_resolve_for_their_own_binding_vocabulary() {
        let cases = [
            (Dialect::Janet, "(each x xs x)"),
            (Dialect::Janet, "(with [f (file/open \"a\")] f)"),
            (Dialect::Janet, "(when-let [x 1] x)"),
            (Dialect::Janet, "(with-syms [a b] a)"),
            (Dialect::Janet, "(fn named [x] x)"),
            (Dialect::Hy, "(for [x xs] x)"),
            (Dialect::Hy, "(with [f (open \"a\")] f)"),
            (Dialect::Carp, "(let-do [x 1] x)"),
            (Dialect::Fennel, "(with-open [f (io.open \"a\")] f)"),
            (Dialect::Fennel, "(lambda [x] x)"),
            (Dialect::Lfe, "(match-lambda ((x) x))"),
        ];

        for (dialect, source) in cases {
            let form = parsed_form(source, dialect);
            assert!(
                DialectSemanticPolicy::new(dialect)
                    .scope_shape(&form)
                    .is_some(),
                "{dialect:?}: {source}"
            );
        }
    }

    #[test]
    fn shallow_dialect_scope_resolvers_fail_closed_on_malformed_forms() {
        let cases = [
            // Janet's `if-let` is deliberately absent: its else branch sits
            // outside the binding's scope, which this vocabulary cannot say.
            (Dialect::Janet, "(if-let [x 1] x 2)"),
            (Dialect::Janet, "(each x)"),
            (Dialect::Janet, "(with-syms [(a)] a)"),
            (Dialect::Janet, "(fn named)"),
            (Dialect::Fennel, "(each [(pairs t)] 1)"),
            (Dialect::Fennel, "(for [] 1)"),
            (Dialect::Fennel, "(each [k v (pairs t)])"),
            (Dialect::Lfe, "(match-lambda (x))"),
        ];

        for (dialect, source) in cases {
            let form = parsed_form(source, dialect);
            assert_eq!(
                DialectSemanticPolicy::new(dialect).scope_shape(&form),
                None,
                "{dialect:?}: {source}"
            );
        }
    }

    #[test]
    fn shallow_dialect_definitions_carry_their_own_categories() {
        let cases = [
            (
                Dialect::Lfe,
                "(defrecord point x y)",
                DefinitionCategory::Struct,
            ),
            (
                Dialect::Carp,
                "(deftype T [a Int])",
                DefinitionCategory::Struct,
            ),
            (
                Dialect::Carp,
                "(defmodule M (def x 1))",
                DefinitionCategory::Package,
            ),
            (
                Dialect::Carp,
                "(definterface f (Fn [a] a))",
                DefinitionCategory::Other,
            ),
            (
                Dialect::Carp,
                "(defndynamic f [x] x)",
                DefinitionCategory::Function,
            ),
            (Dialect::Hy, "(defclass C [] 1)", DefinitionCategory::Class),
            (
                Dialect::Janet,
                "(varfn f [x] x)",
                DefinitionCategory::Function,
            ),
            (Dialect::Janet, "(var x 1)", DefinitionCategory::Variable),
            (
                Dialect::Fennel,
                "(lambda f [x] x)",
                DefinitionCategory::Function,
            ),
            (Dialect::Fennel, "(var x 1)", DefinitionCategory::Variable),
        ];

        for (dialect, source, category) in cases {
            let form = parsed_form(source, dialect);
            assert_eq!(
                DialectSemanticPolicy::new(dialect)
                    .definition_shape(&form)
                    .map(DefinitionShape::category),
                Some(category),
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
    fn scheme_letrec_reports_recursive_visibility() {
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);

        for source in ["(letrec ((f 1)) f)", "(letrec* ((f 1)) f)"] {
            let form = parsed_form(source, Dialect::Scheme);
            let shape = policy.scope_shape(&form).expect("letrec scope");
            assert_eq!(
                shape.binders(),
                BinderShape::BindingList {
                    container: RelativeNodePath::Child(1),
                    name: RelativeNodePath::Child(0),
                    initializer: Some(RelativeNodePath::Child(1)),
                    visibility: BindingVisibility::Recursive,
                },
                "{source}"
            );
        }
    }

    #[test]
    fn scheme_let_family_maps_each_head_to_its_own_visibility() {
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let cases = [
            ("(let ((x 1)) x)", BindingVisibility::Parallel),
            ("(let* ((x 1)) x)", BindingVisibility::Sequential),
            ("(letrec ((x 1)) x)", BindingVisibility::Recursive),
            ("(let-syntax ((x 1)) x)", BindingVisibility::Parallel),
            ("(letrec-syntax ((x 1)) x)", BindingVisibility::Recursive),
        ];

        for (source, visibility) in cases {
            let form = parsed_form(source, Dialect::Scheme);
            let shape = policy.scope_shape(&form).expect("known scope");
            let BinderShape::BindingList {
                visibility: actual, ..
            } = shape.binders()
            else {
                panic!("{source}: expected a binding list");
            };
            assert_eq!(actual, visibility, "{source}");
        }
    }

    #[test]
    fn scheme_binding_lists_may_use_brackets() {
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let form = parsed_form("(let ([x 1]) x)", Dialect::Scheme);

        assert_eq!(
            policy.scope_shape(&form).map(ScopeShape::body),
            Some(BodyShape::ChildrenFrom(2))
        );
    }

    #[test]
    fn scheme_case_lambda_scopes_each_clause_like_a_multi_arity_callable() {
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let form = parsed_form("(case-lambda ((x) x) ((x y) y))", Dialect::Scheme);

        assert_eq!(
            policy.scope_shape(&form),
            Some(ScopeShape::new(
                BinderShape::ParameterClauses {
                    name: None,
                    first_clause_index: 1,
                    parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
                },
                BodyShape::ClauseChildrenFrom {
                    first_clause_index: 1,
                    body_child_index: 1,
                },
            ))
        );
    }

    #[test]
    fn scheme_forms_this_model_cannot_express_report_no_shape() {
        // Each of these binds, and each binds in a way `BinderShape` has no
        // vocabulary for. Returning an approximate shape would be worse than
        // returning none -- a rename driven by one would drop occurrences and
        // emit code naming a variable that no longer exists -- and the
        // reference query handles all of them exactly.
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);

        for source in [
            // A step form per entry, and room for one initializer.
            "(do ((i 0 (+ i 1))) ((= i 3) i))",
            // Bound over the clauses but not over the guarded body.
            "(guard (e (#t e)) (raise 1))",
            // Binds nothing lexically at all.
            "(parameterize ((p 1)) (p))",
        ] {
            let form = parsed_form(source, Dialect::Scheme);
            assert_eq!(policy.scope_shape(&form), None, "{source}");
        }
    }

    #[test]
    fn scheme_let_values_reuses_the_ordinary_binding_list_shape() {
        // The bound position is a formals list rather than one name, which the
        // destructuring readers already handle: every name in a pattern comes
        // back, so `(a b)` binds both.
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let form = parsed_form("(let-values (((a b) (values 1 2))) a)", Dialect::Scheme);

        assert_eq!(
            policy.scope_shape(&form),
            Some(ScopeShape::new(
                scheme_binding_list(BindingVisibility::Parallel),
                BodyShape::ChildrenFrom(2),
            ))
        );
    }

    #[test]
    fn scheme_definition_shapes_cover_the_record_and_syntax_rule_forms() {
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let record = parsed_form(
            "(define-record-type point (make-point x y) point? (x point-x))",
            Dialect::Scheme,
        );
        let syntax_rule = parsed_form("(define-syntax-rule (swap a b) body)", Dialect::Scheme);

        assert_eq!(
            policy
                .definition_shape(&record)
                .map(DefinitionShape::category),
            Some(DefinitionCategory::Struct)
        );

        let shape = policy
            .definition_shape(&syntax_rule)
            .expect("define-syntax-rule shape");
        assert_eq!(shape.category(), DefinitionCategory::Macro);
        assert_eq!(
            shape.name(),
            Some(RelativeNodePath::Grandchild {
                child: 1,
                grandchild: 0,
            })
        );
        assert_eq!(
            shape.parameters(),
            Some(ParameterShape::new(RelativeNodePath::Child(1), 1))
        );
    }

    #[test]
    fn a_curried_define_reports_no_shape_because_its_name_is_too_deep() {
        // `(define ((adder n) x) ...)` puts the name three levels down, one
        // more than `RelativeNodePath` can address. Declining beats reporting
        // `(adder n)` as though it were the name.
        let policy = DialectSemanticPolicy::new(Dialect::Scheme);
        let form = parsed_form("(define ((adder n) x) (+ n x))", Dialect::Scheme);

        assert_eq!(policy.definition_shape(&form), None);
    }

    #[test]
    fn clojure_private_defn_is_a_definition_at_every_layer() {
        // `(defn- helper [x y] ...)` used to fall through all three layers:
        // `inspect outline` marked it as not a definition and `inspect
        // definitions` undercounted functions by one.
        let policy = DialectSemanticPolicy::new(Dialect::Clojure);
        let form = parsed_form("(defn- helper [x y] (+ x y))", Dialect::Clojure);

        assert!(Dialect::Clojure.is_definition_head("defn-"));
        assert_eq!(
            crate::definition::definition_shape(Dialect::Clojure, &form, "defn-")
                .map(|shape| shape.category),
            Some(DefinitionCategory::Function)
        );

        let shape = policy
            .definition_shape(&form)
            .expect("defn- has a definition shape");
        assert_eq!(shape.category(), DefinitionCategory::Function);
        assert_eq!(shape.name(), Some(RelativeNodePath::Child(1)));
        assert_eq!(
            shape.parameters(),
            Some(ParameterShape::new(RelativeNodePath::Child(2), 0))
        );
        assert_eq!(shape.body(), BodyShape::ChildrenFrom(3));

        // `defn-` shares `defn`'s layout exactly.
        let public = parsed_form("(defn helper [x y] (+ x y))", Dialect::Clojure);
        assert_eq!(policy.definition_shape(&public), Some(shape));
    }

    #[test]
    fn clojure_value_definition_shapes_cover_def_and_defonce_but_not_declare() {
        let policy = DialectSemanticPolicy::new(Dialect::Clojure);

        for source in ["(def answer 42)", "(defonce registry (atom {}))"] {
            let form = parsed_form(source, Dialect::Clojure);
            let shape = policy
                .definition_shape(&form)
                .unwrap_or_else(|| panic!("{source} has a definition shape"));
            assert_eq!(shape.category(), DefinitionCategory::Variable, "{source}");
            assert_eq!(shape.name(), Some(RelativeNodePath::Child(1)), "{source}");
        }

        // `declare` names several vars at once, so no single-name shape fits.
        let declare = parsed_form("(declare a b c)", Dialect::Clojure);
        assert_eq!(policy.definition_shape(&declare), None);
    }

    #[test]
    fn clojure_let_scope_is_sequential_because_initializers_see_earlier_names() {
        // `(let [x 1 y (inc x)] y)` is legal Clojure: `y`'s initializer sees
        // `x`. Clojure has no parallel `let`, so this must never report
        // BindingVisibility::Parallel — and the capability layer's
        // `common_lisp_value_scope_form_for_head` must agree with it.
        let form = parsed_form("(let [x 1 y (inc x)] y)", Dialect::Clojure);
        let shape = DialectSemanticPolicy::new(Dialect::Clojure)
            .scope_shape(&form)
            .expect("clojure let scope");

        assert_eq!(shape.binders(), FLAT_BINDINGS_SEQUENTIAL);
        assert!(matches!(
            shape.binders(),
            BinderShape::FlatPairs {
                visibility: BindingVisibility::Sequential,
                ..
            }
        ));
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

#[cfg(test)]
mod clojure_defn_tests {
    use super::*;
    use crate::sexpr::SyntaxTree;

    fn clojure_form(source: &str) -> ExpressionView {
        SyntaxTree::parse_with_dialect(source, Dialect::Clojure)
            .expect("fixture parses")
            .root_view()
            .children
            .first()
            .cloned()
            .expect("fixture has one form")
    }

    fn policy() -> DialectSemanticPolicy {
        DialectSemanticPolicy::new(Dialect::Clojure)
    }

    #[test]
    fn defn_resolves_with_any_combination_of_docstring_and_attribute_map() {
        // A docstring is the norm in idiomatic Clojure, so a shape that only
        // recognises `(defn name [params] body)` misses most definitions.
        for (source, parameters, body_start) in [
            ("(defn f [x] x)", 2, 3),
            (r#"(defn f "doc" [x] x)"#, 3, 4),
            ("(defn f {:added \"1.0\"} [x] x)", 3, 4),
            (r#"(defn f "doc" {:added "1.0"} [x] x)"#, 4, 5),
        ] {
            let shape = policy()
                .definition_shape(&clojure_form(source))
                .unwrap_or_else(|| panic!("no shape for {source}"));
            assert_eq!(shape.category(), DefinitionCategory::Function, "{source}");
            assert_eq!(shape.name(), Some(RelativeNodePath::Child(1)), "{source}");
            assert_eq!(
                shape.parameters(),
                Some(ParameterShape::new(RelativeNodePath::Child(parameters), 0)),
                "{source}"
            );
            assert_eq!(
                shape.body(),
                BodyShape::ChildrenFrom(body_start),
                "{source}"
            );
        }
    }

    #[test]
    fn multi_arity_defn_reports_clause_bodies_and_no_single_parameter_list() {
        for (source, first_clause) in [
            ("(defn f ([x] x) ([x y] y))", 2),
            (r#"(defn f "doc" ([x] x) ([x y] y))"#, 3),
        ] {
            let shape = policy()
                .definition_shape(&clojure_form(source))
                .unwrap_or_else(|| panic!("no shape for {source}"));
            assert_eq!(shape.parameters(), None, "{source}");
            assert_eq!(
                shape.body(),
                BodyShape::ClauseChildrenFrom {
                    first_clause_index: first_clause,
                    body_child_index: 1,
                },
                "{source}"
            );
        }
    }

    #[test]
    fn defn_opens_a_scope_over_its_parameters_but_does_not_bind_its_own_name() {
        // The `defn` name is a namespace-level var, not a lexical binding, so
        // it must not appear in the binder shape the way `(fn add [x] ...)`'s
        // self-reference does.
        let shape = policy()
            .scope_shape(&clojure_form(r#"(defn f "doc" [x] x)"#))
            .expect("defn scope");
        assert_eq!(
            shape.binders(),
            BinderShape::Parameters(ParameterShape::new(RelativeNodePath::Child(3), 0))
        );
        assert_eq!(shape.body(), BodyShape::ChildrenFrom(4));

        let multi = policy()
            .scope_shape(&clojure_form("(defn f ([x] x) ([x y] y))"))
            .expect("multi-arity defn scope");
        assert_eq!(
            multi.binders(),
            BinderShape::ParameterClauses {
                name: None,
                first_clause_index: 2,
                parameters: ParameterShape::new(RelativeNodePath::Child(0), 0),
            }
        );
    }

    #[test]
    fn sequential_binding_forms_share_the_flat_let_shape() {
        for head in [
            "loop",
            "binding",
            "with-open",
            "with-redefs",
            "with-local-vars",
            "if-let",
            "if-some",
            "when-let",
            "when-some",
            "when-first",
        ] {
            let source = format!("({head} [x 1] x)");
            let shape = policy()
                .scope_shape(&clojure_form(&source))
                .unwrap_or_else(|| panic!("no scope for {head}"));
            assert_eq!(shape.binders(), FLAT_BINDINGS_SEQUENTIAL, "{head}");
        }
    }

    #[test]
    fn forms_this_tool_cannot_model_exactly_are_declined_rather_than_approximated() {
        // `doseq`/`for` binding vectors interleave `:let`/`:when`/`:while`
        // modifier clauses with name/sequence pairs, and `letfn` binds
        // `(name [params] body)` lists. No current shape expresses either, and
        // a wrong rename is worse than no rename.
        for source in [
            "(doseq [x xs :when (pos? x)] x)",
            "(for [x xs :let [y (inc x)]] y)",
            "(letfn [(g [a] a)] (g 1))",
        ] {
            assert_eq!(
                policy().scope_shape(&clojure_form(source)),
                None,
                "{source}"
            );
        }
    }

    #[test]
    fn malformed_defn_forms_are_declined() {
        for source in [
            "(defn)",
            "(defn f)",
            "(defn f not-a-vector body)",
            "(defn [x] x)",
            "(defn f ([x] x) malformed)",
        ] {
            assert_eq!(
                policy().definition_shape(&clojure_form(source)),
                None,
                "{source}"
            );
        }
    }
}
