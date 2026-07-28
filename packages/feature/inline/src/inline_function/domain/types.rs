use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path, SymbolName};

/// Which calls to inline.
///
/// Phase 7: this was `call_paths: Vec<Path>` beside `all_calls: bool`, whose
/// combinations include one the command rejects outright - the CLI answers
/// `--all-calls` plus `--call-path` with "accepts either --all-calls or
/// repeated --call-path, not both". A rule enforced at runtime over a state
/// the type permits is a rule that can be forgotten; the enum has no way to
/// write it down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineCallSelection {
    /// Every same-file call to the definition.
    AllCalls,
    /// Exactly the calls at these paths.
    Paths(Vec<Path>),
}

impl InlineCallSelection {
    /// The paths the caller named, or none when it asked for all calls.
    #[must_use]
    pub fn explicit_paths(&self) -> Vec<Path> {
        match self {
            Self::AllCalls => Vec::new(),
            Self::Paths(paths) => paths.clone(),
        }
    }

    /// Whether every call was requested.
    ///
    /// Serialized by `inline-function`'s report as `all_calls`, so it stays
    /// derivable rather than stored a second time.
    #[must_use]
    pub const fn is_all_calls(&self) -> bool {
        matches!(self, Self::AllCalls)
    }
}

#[derive(Debug, Clone)]
pub struct InlineFunctionRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub calls: InlineCallSelection,
    pub remove_definition: bool,
    pub allow_duplicate_evaluation: bool,
    pub allow_drop_arguments: bool,
}

#[derive(Debug, Clone)]
pub struct InlineFunctionPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub calls: Vec<InlineFunctionCallPlan>,
    pub remove_definition: bool,
    pub definition_removed: bool,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFunctionCallPlan {
    pub call_path: Path,
    pub call_span: ByteSpan,
    pub parameters: Vec<InlineFunctionParameterPlan>,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFunctionParameterPlan {
    pub name: String,
    pub argument: String,
    pub reference_count: usize,
}
