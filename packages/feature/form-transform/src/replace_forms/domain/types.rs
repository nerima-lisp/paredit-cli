use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::form_shape::FormShape;
use paredit_core_syntax::sexpr::{ByteSpan, Path, SyntaxTree};

#[derive(Debug)]
pub struct ReplaceFormsRequest<'a> {
    pub input: &'a str,
    pub tree: &'a SyntaxTree,
    pub dialect: Dialect,
    pub paths: Vec<Path>,
    pub replacement: &'a str,
    pub require_same_shape: bool,
}

#[derive(Debug)]
pub struct ReplaceFormsPlan {
    pub targets: Vec<ReplaceFormsTarget>,
    pub replacement: String,
    pub replacement_shape: FormShape,
    pub require_same_shape: bool,
    pub original_shape: Option<FormShape>,
    pub changed: bool,
    pub rewritten: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceFormsTarget {
    pub form_path: Path,
    pub span: ByteSpan,
    pub shape: FormShape,
    pub text: String,
}
