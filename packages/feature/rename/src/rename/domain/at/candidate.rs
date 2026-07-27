use anyhow::Result;

use super::RenameAtNamespace;
use super::selection::AtomPathIndex;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree};

mod global_callable;
mod lexical_value;
mod scope;
mod scoped_callable;
mod symbol_macro;

pub(super) struct Candidate {
    pub(super) namespace: RenameAtNamespace,
    pub(super) occurrences: Vec<ByteSpan>,
    pub(super) rewritten: String,
}

pub(super) struct SpecializedCandidateContext<'a> {
    pub(super) input: &'a str,
    pub(super) dialect: Dialect,
    pub(super) tree: &'a SyntaxTree,
    pub(super) root_view: &'a ExpressionView,
    pub(super) atom_paths: AtomPathIndex<'a>,
    pub(super) path: &'a Path,
    pub(super) selected_span: ByteSpan,
    pub(super) from: &'a SymbolName,
    pub(super) to: &'a SymbolName,
}

pub(in crate::domain::rename::at) use lexical_value::binding_candidates;

pub(super) fn add_specialized_candidates(
    output: &mut Vec<Candidate>,
    context: SpecializedCandidateContext<'_>,
) -> Result<()> {
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
