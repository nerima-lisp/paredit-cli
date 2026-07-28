use super::SchemeOperator;

/// Resolves an operator head.
///
/// Scheme is case-sensitive (R7RS 2.1), so this is an exact match -- unlike
/// the Common Lisp table, which folds case to match the default readtable.
pub(super) fn scheme_operator_from_head(head: &str) -> Option<SchemeOperator> {
    Some(match head {
        "let" => SchemeOperator::Let,
        "let*" => SchemeOperator::LetStar,
        "letrec" => SchemeOperator::Letrec,
        "letrec*" => SchemeOperator::LetrecStar,
        "let-syntax" => SchemeOperator::LetSyntax,
        "letrec-syntax" => SchemeOperator::LetrecSyntax,
        "let-values" => SchemeOperator::LetValues,
        "let*-values" => SchemeOperator::LetStarValues,
        "letrec-values" => SchemeOperator::LetrecValues,
        "do" => SchemeOperator::Do,
        // `λ` is Racket's reader-level synonym for `lambda`, and Guile and
        // Chez accept it too.
        "lambda" | "λ" => SchemeOperator::Lambda,
        "case-lambda" => SchemeOperator::CaseLambda,
        "guard" => SchemeOperator::Guard,
        "parameterize" => SchemeOperator::Parameterize,
        "fluid-let" => SchemeOperator::FluidLet,

        "define" => SchemeOperator::Define,
        "define-values" => SchemeOperator::DefineValues,
        "define-record-type" => SchemeOperator::DefineRecordType,
        "define-syntax" => SchemeOperator::DefineSyntax,
        "define-syntax-rule" => SchemeOperator::DefineSyntaxRule,
        "define-library" | "library" => SchemeOperator::DefineLibrary,
        "struct" => SchemeOperator::Struct,
        "define-struct" => SchemeOperator::DefineStruct,
        "define/contract" => SchemeOperator::DefineContract,

        "begin" => SchemeOperator::Begin,
        "when" => SchemeOperator::When,
        "unless" => SchemeOperator::Unless,
        "cond" => SchemeOperator::Cond,
        "case" => SchemeOperator::Case,
        "if" => SchemeOperator::If,
        "and" => SchemeOperator::And,
        "or" => SchemeOperator::Or,
        "delay" => SchemeOperator::Delay,
        "delay-force" | "make-promise-lazy" => SchemeOperator::DelayForce,
        "make-promise" => SchemeOperator::MakePromise,

        "set!" => SchemeOperator::Set,

        "syntax-rules" => SchemeOperator::SyntaxRules,
        "syntax-case" => SchemeOperator::SyntaxCase,

        "import" => SchemeOperator::Import,
        "export" => SchemeOperator::Export,
        "include" => SchemeOperator::Include,
        "include-ci" => SchemeOperator::IncludeCi,
        "include-library-declarations" => SchemeOperator::IncludeLibraryDeclarations,
        "cond-expand" => SchemeOperator::CondExpand,

        _ => return None,
    })
}
