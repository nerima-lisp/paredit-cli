//! Dialect-neutral descriptions of the Emacs Lisp form shapes this tool reads.
//!
//! These say *where* the parts of a form sit, not what they mean. A consumer
//! that knows a `(let BINDINGS BODY…)` shape can walk `if-let*`, `dlet`, and
//! `pcase-let*` without a branch per operator, which is the whole point: the
//! `subr-x` and `cl-lib` families keep adding spellings of the same three or
//! four layouts.

/// Whether a binding form's initializers can see the names bound before them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EmacsLispBindingVisibility {
    /// Every initializer is evaluated in the enclosing scope (`let`).
    Parallel,
    /// Each initializer sees the preceding bindings (`let*`, `if-let*`).
    Sequential,
    /// Every name is visible to every initializer, including its own
    /// (`letrec`, `cl-labels`).
    Recursive,
}

impl EmacsLispBindingVisibility {
    #[must_use]
    pub const fn is_sequential(self) -> bool {
        matches!(self, Self::Sequential | Self::Recursive)
    }
}

/// Whether a `let`-shaped form binds lexically or dynamically.
///
/// Emacs Lisp answers this per *file* for `let` — the `lexical-binding` file
/// header decides — and per *form* for `dlet`, which is dynamic regardless.
/// The distinction is not cosmetic: a dynamically bound name is readable by
/// every function the body calls, so "this binding is never referenced in the
/// body" is not evidence that it is dead.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EmacsLispBindingScope {
    /// Lexical when the file header says so, dynamic otherwise.
    FileDefault,
    /// Always dynamic, whatever the header says (`dlet`).
    AlwaysDynamic,
    /// Always lexical (`cl-flet`, `lambda` parameters under `lexical-binding`).
    AlwaysLexical,
}

/// How the binders of a scope-opening form are laid out.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EmacsLispBinderShape {
    /// `((name value) …)` or bare `name` entries in a list at `container`.
    PairList {
        /// Child index of the binding list.
        container: usize,
        /// Child index at which the body starts.
        body_start: usize,
        /// Child index at which the bindings stop being visible, for the
        /// `if-let*` family: its ELSE forms are siblings of the THEN form but
        /// are evaluated with none of the bindings in scope. `None` means the
        /// bindings are visible to the end of the form.
        body_end: Option<usize>,
        visibility: EmacsLispBindingVisibility,
    },
    /// `((PATTERN value) …)`, where each name comes from a `pcase` pattern.
    PatternPairList {
        container: usize,
        body_start: usize,
        visibility: EmacsLispBindingVisibility,
    },
    /// `(seq-let PATTERN value BODY…)`: one destructuring pattern, not a list.
    SinglePattern {
        /// Child index of the pattern.
        pattern: usize,
        /// Child index of the value form, evaluated in the enclosing scope.
        value: usize,
        body_start: usize,
    },
    /// `(cl-destructuring-bind ARGLIST value BODY…)`: an arglist pattern.
    ArgumentList {
        arglist: usize,
        value: usize,
        body_start: usize,
    },
    /// `(dolist (VAR LIST [RESULT]) BODY…)`.
    Iteration {
        /// Child index of the `(VAR LIST [RESULT])` spec.
        spec: usize,
        body_start: usize,
        /// Whether the spec's third element is a result form evaluated with
        /// the loop variable still bound.
        has_result_form: bool,
    },
    /// `(pcase-dolist (PATTERN LIST) BODY…)`.
    PatternIteration { spec: usize, body_start: usize },
    /// `(cl-do ((var init step) …) (END RESULT…) BODY…)`.
    VariableSpecs {
        container: usize,
        body_start: usize,
        visibility: EmacsLispBindingVisibility,
    },
    /// `(cl-flet ((name ARGS BODY…) …) BODY…)`.
    LocalCallables {
        container: usize,
        body_start: usize,
        form: EmacsLispLocalCallableForm,
    },
    /// `(named-let NAME ((var init) …) BODY…)`.
    NamedLet {
        name: usize,
        container: usize,
        body_start: usize,
    },
    /// `(condition-case VAR BODYFORM HANDLERS…)`: `VAR` is bound only inside
    /// the handler clauses, never inside `BODYFORM`.
    ConditionCase {
        variable: usize,
        protected: usize,
        first_handler: usize,
    },
    /// `(lambda ARGS BODY…)`.
    Parameters { arglist: usize, body_start: usize },
    /// `(with-slots SPECS OBJECT BODY…)`.
    Slots {
        container: usize,
        object: usize,
        body_start: usize,
    },
}

impl EmacsLispBinderShape {
    /// The child index after the last form the bindings are visible in, or
    /// `None` when they are visible to the end of the form.
    #[must_use]
    pub const fn body_end(self) -> Option<usize> {
        match self {
            Self::PairList { body_end, .. } => body_end,
            _ => None,
        }
    }

    /// The child index at which body forms begin, for the shapes that have a
    /// single contiguous body.
    #[must_use]
    pub const fn body_start(self) -> Option<usize> {
        match self {
            Self::PairList { body_start, .. }
            | Self::PatternPairList { body_start, .. }
            | Self::SinglePattern { body_start, .. }
            | Self::ArgumentList { body_start, .. }
            | Self::Iteration { body_start, .. }
            | Self::PatternIteration { body_start, .. }
            | Self::VariableSpecs { body_start, .. }
            | Self::LocalCallables { body_start, .. }
            | Self::NamedLet { body_start, .. }
            | Self::Parameters { body_start, .. }
            | Self::Slots { body_start, .. } => Some(body_start),
            Self::ConditionCase { .. } => None,
        }
    }
}

/// The `cl-lib` local-callable forms and their visibility rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EmacsLispLocalCallableForm {
    /// `cl-flet`: definitions close over the enclosing scope only.
    Flet,
    /// `cl-flet*`: each definition sees the ones before it.
    FletStar,
    /// `cl-labels`: every definition sees the whole group, so they recurse.
    Labels,
    /// `cl-macrolet`: local macros, same visibility as `cl-labels`.
    Macrolet,
    /// `cl.el`'s `flet`, which rebound the function cell dynamically.
    ObsoleteDynamicFlet,
    /// `cl.el`'s `labels`.
    ObsoleteDynamicLabels,
}

impl EmacsLispLocalCallableForm {
    #[must_use]
    pub const fn is_macro(self) -> bool {
        matches!(self, Self::Macrolet)
    }

    /// Whether a definition in the group can see its siblings and itself.
    #[must_use]
    pub const fn group_is_self_visible(self) -> bool {
        matches!(
            self,
            Self::Labels | Self::Macrolet | Self::ObsoleteDynamicLabels
        )
    }

    /// Whether each definition sees only the definitions written before it.
    #[must_use]
    pub const fn is_sequential(self) -> bool {
        matches!(self, Self::FletStar)
    }

    #[must_use]
    pub const fn operator_name(self) -> &'static str {
        match self {
            Self::Flet => "cl-flet",
            Self::FletStar => "cl-flet*",
            Self::Labels => "cl-labels",
            Self::Macrolet => "cl-macrolet",
            Self::ObsoleteDynamicFlet => "flet",
            Self::ObsoleteDynamicLabels => "labels",
        }
    }
}

/// A form that ties this file to another library at load or compile time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EmacsLispDependencyForm {
    /// `(require 'FEATURE [FILENAME [NOERROR]])`.
    Require,
    /// `(provide 'FEATURE)`.
    Provide,
    /// `(load FILE …)`.
    Load,
    /// `(load-file FILE)`.
    LoadFile,
    /// `(load-library LIBRARY)`.
    LoadLibrary,
    /// `(autoload 'FUNCTION FILE …)`.
    Autoload,
    /// `(declare-function FUNCTION FILE …)`: a byte-compiler promise, not a
    /// load.
    DeclareFunction,
    /// `(define-package NAME VERSION …)` in a generated `-pkg.el`.
    DefinePackage,
}

impl EmacsLispDependencyForm {
    /// Child index of the designator naming the other library, when the form
    /// has one at a fixed position.
    #[must_use]
    pub const fn designator_child_index(self) -> usize {
        match self {
            Self::Require | Self::Provide | Self::Load | Self::LoadFile | Self::LoadLibrary => 1,
            // `(autoload 'FUNCTION FILE)` and `(declare-function F FILE)`
            // both name the function first and the file second.
            Self::Autoload | Self::DeclareFunction | Self::DefinePackage => 2,
        }
    }

    /// Whether the form makes the named library available at load time.
    ///
    /// `autoload` and `declare-function` do not: the first defers the load
    /// until the function is called, the second only silences a warning.
    #[must_use]
    pub const fn loads_eagerly(self) -> bool {
        matches!(
            self,
            Self::Require | Self::Load | Self::LoadFile | Self::LoadLibrary
        )
    }
}

/// The kind of thing a `def…` form introduces, at the granularity Emacs Lisp
/// itself distinguishes when it stores the definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EmacsLispDefinitionCell {
    /// Stored in the symbol's function cell.
    Function,
    /// Stored in the function cell, but expanded at compile time.
    Macro,
    /// Stored in the value cell.
    Variable,
    /// A `defcustom`/`defface`/`defgroup` customization item.
    Customization,
    /// A `cl-defstruct`/`cl-deftype`/`defclass` type.
    Type,
    /// A major or minor mode, which defines several symbols at once.
    Mode,
    /// A `define-error` condition.
    Condition,
}

/// Where a definition's lambda list sits, and where its body begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EmacsLispCallableShape {
    arglist_child_index: usize,
    body_start_child_index: usize,
    accepts_docstring: bool,
    accepts_declare: bool,
    accepts_interactive: bool,
}

impl EmacsLispCallableShape {
    pub(super) const fn new(
        arglist_child_index: usize,
        accepts_docstring: bool,
        accepts_declare: bool,
        accepts_interactive: bool,
    ) -> Self {
        Self {
            arglist_child_index,
            body_start_child_index: arglist_child_index + 1,
            accepts_docstring,
            accepts_declare,
            accepts_interactive,
        }
    }

    #[must_use]
    pub const fn arglist_child_index(self) -> usize {
        self.arglist_child_index
    }

    #[must_use]
    pub const fn body_start_child_index(self) -> usize {
        self.body_start_child_index
    }

    /// Whether a leading string in the body is a docstring rather than a
    /// value the form evaluates and discards.
    #[must_use]
    pub const fn accepts_docstring(self) -> bool {
        self.accepts_docstring
    }

    /// Whether a leading `(declare …)` in the body is metadata rather than a
    /// call.
    #[must_use]
    pub const fn accepts_declare(self) -> bool {
        self.accepts_declare
    }

    /// Whether a leading `(interactive …)` in the body is a command
    /// specification rather than a call.
    #[must_use]
    pub const fn accepts_interactive(self) -> bool {
        self.accepts_interactive
    }
}
