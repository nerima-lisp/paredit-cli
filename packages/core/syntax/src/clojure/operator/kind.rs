/// A recognized Clojure special form, macro, or definition head.
///
/// Clojure is case-sensitive and has no reader-case folding, so unlike
/// [`crate::common_lisp::CommonLispOperator`] every head here matches exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ClojureOperator {
    // Namespace and vars.
    Ns,
    InNs,
    Def,
    Defonce,
    Declare,
    Defn,
    DefnPrivate,
    Defmacro,
    Defmulti,
    Defmethod,
    Defprotocol,
    Definterface,
    Defrecord,
    Deftype,
    Defstruct,

    // clojure.test.
    Deftest,
    Testing,
    UseFixtures,

    // Lexical scopes.
    Let,
    Letfn,
    Loop,
    Binding,
    WithLocalVars,
    WithOpen,
    WithRedefs,
    WithOutStr,
    WhenLet,
    WhenSome,
    WhenFirst,
    IfLet,
    IfSome,
    Doseq,
    Dotimes,
    For,
    Fn,
    Reify,
    ExtendType,
    ExtendProtocol,
    Proxy,

    // Control flow.
    If,
    IfNot,
    When,
    WhenNot,
    Cond,
    Condp,
    Case,
    Do,
    Doto,
    Try,
    Catch,
    Finally,
    While,
    Locking,
    Comment,

    // Threading and pipelines.
    Thread,
    ThreadLast,
    AsThread,
    SomeThread,
    SomeThreadLast,
    CondThread,
    CondThreadLast,
}

/// The evaluation order of a Clojure binding form's initializers.
///
/// Every Clojure binding form is sequential: unlike Common Lisp, there is no
/// parallel `let`. This type exists so callers state that fact explicitly
/// rather than borrowing Common Lisp's parallel/sequential distinction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ClojureBindingForm {
    /// A `[name init name init ...]` vector of alternating pairs, as in `let`.
    SequentialPairs,
    /// A `[name init ...]` vector whose names are local function bindings
    /// visible to each other, as in `letfn`.
    LocalFunctions,
    /// A `[name seq-expr ...]` vector of iteration clauses, as in `doseq`.
    IterationClauses,
    /// A parameter vector, as in `fn`.
    Parameters,
}

/// The argument-threading discipline of a Clojure threading macro.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ClojureThreadingForm {
    /// `->`: inserts the value as the first argument of each step.
    First,
    /// `->>`: inserts the value as the last argument of each step.
    Last,
    /// `as->`: binds the value to a name reused positionally in each step.
    Named,
    /// `some->`: like `First`, short-circuiting on `nil`.
    SomeFirst,
    /// `some->>`: like `Last`, short-circuiting on `nil`.
    SomeLast,
    /// `cond->`: like `First`, applying only the steps whose test passes.
    CondFirst,
    /// `cond->>`: like `Last`, applying only the steps whose test passes.
    CondLast,
}

impl ClojureThreadingForm {
    /// Returns the stable CLI and JSON identifier for this threading form.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::First => "->",
            Self::Last => "->>",
            Self::Named => "as->",
            Self::SomeFirst => "some->",
            Self::SomeLast => "some->>",
            Self::CondFirst => "cond->",
            Self::CondLast => "cond->>",
        }
    }

    /// Returns whether steps receive the threaded value as their last argument.
    #[must_use]
    pub const fn threads_last(self) -> bool {
        matches!(self, Self::Last | Self::SomeLast | Self::CondLast)
    }
}
