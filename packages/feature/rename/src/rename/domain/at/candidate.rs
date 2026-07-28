use crate::error::RenameResult;

use super::RenameAtNamespace;
use super::selection::AtomPathIndex;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree};

mod global_callable;
mod lexical_value;
mod scope;
mod scoped_callable;
mod symbol_macro;

pub struct Candidate {
    pub namespace: RenameAtNamespace,
    pub occurrences: Vec<ByteSpan>,
    pub rewritten: String,
}

pub struct SpecializedCandidateContext<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub tree: &'a SyntaxTree,
    pub root_view: &'a ExpressionView,
    pub atom_paths: AtomPathIndex<'a>,
    pub path: &'a Path,
    pub selected_span: ByteSpan,
    pub from: &'a SymbolName,
    pub to: &'a SymbolName,
}

pub use lexical_value::binding_candidates;

pub fn add_specialized_candidates(
    output: &mut Vec<Candidate>,
    context: SpecializedCandidateContext<'_>,
) -> RenameResult<()> {
    global_callable::add(output, &context)?;
    scoped_callable::add_local_function(output, &context)?;
    scoped_callable::add_macro(output, &context)?;
    symbol_macro::add(output, &context)?;
    Ok(())
}

fn push_candidate(
    output: &mut Vec<Candidate>,
    namespace: RenameAtNamespace,
    selected_span: ByteSpan,
    has_unique_definition: bool,
    occurrences: Vec<ByteSpan>,
    rewritten: String,
) {
    if has_unique_definition && occurrences.contains(&selected_span) {
        output.push(Candidate {
            namespace,
            occurrences,
            rewritten,
        });
    }
}
