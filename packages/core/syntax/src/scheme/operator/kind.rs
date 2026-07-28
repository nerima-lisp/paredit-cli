/// A Scheme or Racket operator this crate can reason about structurally.
///
/// Membership is deliberately narrow: an operator earns a variant only when
/// some layer needs to know its *shape*. `car` and `display` are ordinary
/// procedures with nothing structural to say, so they are absent, and a head
/// that resolves to `None` is treated as an unknown call everywhere.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SchemeOperator {
    // Binding forms.
    Let,
    LetStar,
    Letrec,
    LetrecStar,
    LetSyntax,
    LetrecSyntax,
    LetValues,
    LetStarValues,
    LetrecValues,
    Do,
    Lambda,
    CaseLambda,
    Guard,
    Parameterize,
    FluidLet,

    // Definition forms.
    Define,
    DefineValues,
    DefineRecordType,
    DefineSyntax,
    DefineSyntaxRule,
    DefineLibrary,
    Struct,
    DefineStruct,
    DefineContract,

    // Sequencing and control whose body is ordinary code.
    Begin,
    When,
    Unless,
    Cond,
    Case,
    If,
    And,
    Or,
    Delay,
    DelayForce,
    MakePromise,

    // Assignment.
    Set,

    // Syntax transformers.
    SyntaxRules,
    SyntaxCase,

    // Library declarations.
    Import,
    Export,
    Include,
    IncludeCi,
    IncludeLibraryDeclarations,
    CondExpand,
}
