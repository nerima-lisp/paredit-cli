use super::EmacsLispOperator;

/// Resolves an Emacs Lisp operator head.
///
/// Matching is exact. Emacs Lisp reads symbols case-sensitively and has no
/// package system, so neither the case folding nor the `cl:` prefix stripping
/// that [`crate::common_lisp::normalize_common_lisp_operator_head`] performs
/// applies here: in a `.el` file `LET` and `let` are two different symbols,
/// and `cl:let` is one symbol whose name happens to contain a colon.
///
/// The `cl-lib` names are spelled out rather than derived by stripping a
/// `cl-` prefix, because the prefix is not a namespace — `cl-defun` and
/// `defun` take different lambda lists, and a user function named
/// `cl-my-helper` must not resolve to anything at all.
pub(super) fn emacs_lisp_operator_from_head(head: &str) -> Option<EmacsLispOperator> {
    Some(match head {
        "let" => EmacsLispOperator::Let,
        "let*" => EmacsLispOperator::LetStar,
        "letrec" => EmacsLispOperator::Letrec,
        "dlet" => EmacsLispOperator::Dlet,
        "named-let" => EmacsLispOperator::NamedLet,
        "cl-letf" | "letf" => EmacsLispOperator::ClLetf,
        "cl-letf*" | "letf*" => EmacsLispOperator::ClLetfStar,
        "cl-symbol-macrolet" | "symbol-macrolet" => EmacsLispOperator::ClSymbolMacrolet,

        "if-let" => EmacsLispOperator::IfLet,
        "if-let*" => EmacsLispOperator::IfLetStar,
        "when-let" => EmacsLispOperator::WhenLet,
        "when-let*" => EmacsLispOperator::WhenLetStar,
        "and-let*" => EmacsLispOperator::AndLetStar,
        "while-let" => EmacsLispOperator::WhileLet,

        "cl-destructuring-bind" | "destructuring-bind" => EmacsLispOperator::ClDestructuringBind,
        "cl-multiple-value-bind" | "multiple-value-bind" => EmacsLispOperator::ClMultipleValueBind,
        "pcase-let" => EmacsLispOperator::PcaseLet,
        "pcase-let*" => EmacsLispOperator::PcaseLetStar,
        "seq-let" => EmacsLispOperator::SeqLet,

        "dolist" => EmacsLispOperator::Dolist,
        "dotimes" => EmacsLispOperator::Dotimes,
        "cl-dolist" => EmacsLispOperator::ClDolist,
        "cl-dotimes" => EmacsLispOperator::ClDotimes,
        "pcase-dolist" => EmacsLispOperator::PcaseDolist,
        "cl-do" | "do" => EmacsLispOperator::ClDo,
        "cl-do*" | "do*" => EmacsLispOperator::ClDoStar,
        "cl-loop" | "loop" => EmacsLispOperator::ClLoop,

        "cl-flet" => EmacsLispOperator::ClFlet,
        "cl-flet*" => EmacsLispOperator::ClFletStar,
        "cl-labels" => EmacsLispOperator::ClLabels,
        "cl-macrolet" | "macrolet" => EmacsLispOperator::ClMacrolet,
        // `cl.el`'s `flet` bound the *function cell* dynamically rather than
        // lexically, so it is a distinct variant even though `cl-flet`
        // superseded it. Files old enough to use it still parse.
        "flet" => EmacsLispOperator::Flet,
        "labels" => EmacsLispOperator::Labels,

        "condition-case" => EmacsLispOperator::ConditionCase,
        "condition-case-unless-debug" => EmacsLispOperator::ConditionCaseUnlessDebug,
        "with-slots" => EmacsLispOperator::WithSlots,

        "lambda" => EmacsLispOperator::Lambda,
        "closure" => EmacsLispOperator::Closure,
        "cl-function" => EmacsLispOperator::ClFunction,

        "defun" => EmacsLispOperator::Defun,
        "defsubst" => EmacsLispOperator::Defsubst,
        "defmacro" => EmacsLispOperator::Defmacro,
        "define-inline" => EmacsLispOperator::DefineInline,
        "cl-defun" => EmacsLispOperator::ClDefun,
        "cl-defsubst" => EmacsLispOperator::ClDefsubst,
        "cl-defmacro" => EmacsLispOperator::ClDefmacro,
        "cl-defgeneric" => EmacsLispOperator::ClDefgeneric,
        "cl-defmethod" => EmacsLispOperator::ClDefmethod,
        "cl-define-compiler-macro" => EmacsLispOperator::ClDefineCompilerMacro,
        "defadvice" => EmacsLispOperator::Defadvice,
        "define-advice" => EmacsLispOperator::DefineAdvice,
        "defalias" => EmacsLispOperator::Defalias,
        "define-obsolete-function-alias" => EmacsLispOperator::DefineObsoleteFunctionAlias,
        "declare-function" => EmacsLispOperator::DeclareFunction,
        "ert-deftest" => EmacsLispOperator::ErtDeftest,

        "cl-defstruct" => EmacsLispOperator::ClDefstruct,
        "cl-deftype" => EmacsLispOperator::ClDeftype,
        "define-error" => EmacsLispOperator::DefineError,
        "defclass" => EmacsLispOperator::Defclass,

        "defvar" => EmacsLispOperator::Defvar,
        "defvar-local" => EmacsLispOperator::DefvarLocal,
        "defvar-keymap" => EmacsLispOperator::DefvarKeymap,
        "defconst" => EmacsLispOperator::Defconst,
        "defvaralias" => EmacsLispOperator::Defvaralias,
        "define-obsolete-variable-alias" => EmacsLispOperator::DefineObsoleteVariableAlias,

        "defcustom" => EmacsLispOperator::Defcustom,
        "defface" => EmacsLispOperator::Defface,
        "defgroup" => EmacsLispOperator::Defgroup,
        "deftheme" => EmacsLispOperator::Deftheme,
        "define-widget" => EmacsLispOperator::DefineWidget,

        "define-minor-mode" => EmacsLispOperator::DefineMinorMode,
        "define-derived-mode" => EmacsLispOperator::DefineDerivedMode,
        "define-globalized-minor-mode" => EmacsLispOperator::DefineGlobalizedMinorMode,
        "define-global-minor-mode" => EmacsLispOperator::DefineGlobalMinorMode,
        "define-generic-mode" => EmacsLispOperator::DefineGenericMode,
        "define-compilation-mode" => EmacsLispOperator::DefineCompilationMode,

        "require" => EmacsLispOperator::Require,
        "provide" => EmacsLispOperator::Provide,
        "load" => EmacsLispOperator::Load,
        "load-file" => EmacsLispOperator::LoadFile,
        "load-library" => EmacsLispOperator::LoadLibrary,
        "autoload" => EmacsLispOperator::Autoload,
        "define-package" => EmacsLispOperator::DefinePackage,

        "eval-when-compile" => EmacsLispOperator::EvalWhenCompile,
        "eval-and-compile" => EmacsLispOperator::EvalAndCompile,
        "with-no-warnings" => EmacsLispOperator::WithNoWarnings,
        "with-suppressed-warnings" => EmacsLispOperator::WithSuppressedWarnings,

        "progn" => EmacsLispOperator::Progn,
        "prog1" => EmacsLispOperator::Prog1,
        "prog2" => EmacsLispOperator::Prog2,
        "cl-block" | "block" => EmacsLispOperator::ClBlock,
        "cl-return-from" | "return-from" => EmacsLispOperator::ClReturnFrom,
        "catch" => EmacsLispOperator::Catch,
        "unwind-protect" => EmacsLispOperator::UnwindProtect,
        "save-excursion" => EmacsLispOperator::SaveExcursion,
        "save-restriction" => EmacsLispOperator::SaveRestriction,
        "save-match-data" => EmacsLispOperator::SaveMatchData,
        "save-current-buffer" => EmacsLispOperator::SaveCurrentBuffer,
        "save-window-excursion" => EmacsLispOperator::SaveWindowExcursion,
        "with-current-buffer" => EmacsLispOperator::WithCurrentBuffer,
        "with-temp-buffer" => EmacsLispOperator::WithTempBuffer,
        "with-temp-file" => EmacsLispOperator::WithTempFile,
        "with-output-to-string" => EmacsLispOperator::WithOutputToString,
        "with-output-to-temp-buffer" => EmacsLispOperator::WithOutputToTempBuffer,
        "with-silent-modifications" => EmacsLispOperator::WithSilentModifications,
        "with-selected-window" => EmacsLispOperator::WithSelectedWindow,
        "with-syntax-table" => EmacsLispOperator::WithSyntaxTable,

        "declare" => EmacsLispOperator::Declare,
        "cl-declare" => EmacsLispOperator::ClDeclare,
        "interactive" => EmacsLispOperator::Interactive,

        "pcase" => EmacsLispOperator::Pcase,
        "pcase-exhaustive" => EmacsLispOperator::PcaseExhaustive,
        "cl-case" | "case" => EmacsLispOperator::ClCase,
        "cl-ecase" | "ecase" => EmacsLispOperator::ClEcase,
        "cl-typecase" | "typecase" => EmacsLispOperator::ClTypecase,
        "cl-etypecase" | "etypecase" => EmacsLispOperator::ClEtypecase,

        _ => return None,
    })
}
