use paredit_core_cli::CliResult;
use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_cli::error::IoRefusal;
use paredit_core_cli::shared::{apply_byte_span_edits, stable_text_hash};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, Edit, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkRequest {
    pub version: u32,
    pub input_hash: String,
    pub operations: Vec<BulkOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum BulkOperation {
    Wrap {
        path: String,
        delimiter: BulkDelimiter,
    },
    RenameInForm {
        path: String,
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BulkDelimiter {
    Paren,
    Bracket,
    Brace,
}

impl From<BulkDelimiter> for Delimiter {
    fn from(value: BulkDelimiter) -> Self {
        match value {
            BulkDelimiter::Paren => Self::Paren,
            BulkDelimiter::Bracket => Self::Bracket,
            BulkDelimiter::Brace => Self::Brace,
        }
    }
}

#[derive(Debug, Error)]
pub enum BulkRefusal {
    #[error("bulk request version must be 1")]
    Version,
    #[error("bulk request must contain at least one operation")]
    Empty,
    #[error("bulk input_hash must be fnv1a64: followed by 16 lowercase hexadecimal digits")]
    HashShape,
    #[error("bulk source changed since the request was prepared")]
    Stale,
    #[error("bulk requires a known dialect")]
    UnknownDialect,
    #[error("bulk paths must select an expression, not the document root")]
    RootPath,
    #[error("bulk tree does not describe the supplied source")]
    TreeMismatch,
    #[error("bulk rename name does not reparse as a single unprefixed atom")]
    InvalidSymbol,
}

paredit_core_cli::impl_classified_refusal!(BulkRefusal, |error| match error {
    BulkRefusal::Stale => ErrorCode::RefusalTargetChanged,
    BulkRefusal::UnknownDialect => ErrorCode::InputDialectUnsupported,
    _ => ErrorCode::InputShapeRefused,
});

#[derive(Debug)]
pub struct BulkPlan {
    pub rewritten: String,
    pub tree: SyntaxTree,
    pub operation_count: usize,
    pub edit_count: usize,
    pub forms_before: usize,
    pub changed: bool,
}

pub fn plan_bulk(
    input: &str,
    tree: &SyntaxTree,
    dialect: Dialect,
    request: BulkRequest,
) -> CliResult<BulkPlan> {
    if request.version != 1 {
        return Err(BulkRefusal::Version.into());
    }
    if request.operations.is_empty() {
        return Err(BulkRefusal::Empty.into());
    }
    let valid_hash = request
        .input_hash
        .strip_prefix("fnv1a64:")
        .is_some_and(|hash| {
            hash.len() == 16
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        });
    if !valid_hash {
        return Err(BulkRefusal::HashShape.into());
    }
    if stable_text_hash(input) != request.input_hash {
        return Err(BulkRefusal::Stale.into());
    }
    if dialect == Dialect::Unknown {
        return Err(BulkRefusal::UnknownDialect.into());
    }
    if tree.source() != input {
        return Err(BulkRefusal::TreeMismatch.into());
    }

    let mut edits: Vec<(ByteSpan, String)> = Vec::new();
    for operation in &request.operations {
        let path_text = match operation {
            BulkOperation::Wrap { path, .. } | BulkOperation::RenameInForm { path, .. } => path,
        };
        let path: Path = path_text.parse()?;
        if path.indexes().is_empty() {
            return Err(BulkRefusal::RootPath.into());
        }
        // Every selection and lowering reads this original tree, never an
        // intermediate rewrite whose paths may have shifted.
        let selection = tree.select_path(&path)?;
        match operation {
            BulkOperation::Wrap { delimiter, .. } => {
                let span = selection.span();
                let wrapped = Edit::wrap(input, tree, selection, (*delimiter).into())?;
                let end = wrapped.len() - (input.len() - span.end().get());
                edits.push((span, wrapped[span.start().get()..end].to_owned()));
            }
            BulkOperation::RenameInForm { from, to, .. } => {
                let from = rename_symbol(from, dialect)?;
                let to = rename_symbol(to, dialect)?;
                for span in rename_occurrences(&selection.view(), &from, dialect) {
                    if &input[span.start().get()..span.end().get()] != to.as_str() {
                        edits.push((span, to.as_str().to_owned()));
                    }
                }
            }
        }
    }
    let edit_count = edits.len();
    // Compare actual replacement spans, not selected rename scopes: an
    // ancestor rename scope can still be disjoint from a wrapped descendant.
    let rewritten = apply_byte_span_edits(input, edits)?;
    let rewritten_tree = SyntaxTree::parse_with_dialect(&rewritten, dialect)
        .map_err(|_| IoRefusal::RewriteDoesNotReparse)?;
    Ok(BulkPlan {
        changed: rewritten != input,
        rewritten,
        tree: rewritten_tree,
        operation_count: request.operations.len(),
        edit_count,
        forms_before: tree.root_view().children.len(),
    })
}

fn rename_symbol(text: &str, dialect: Dialect) -> CliResult<SymbolName> {
    let name = SymbolName::new(text.to_owned())?;
    // A standalone parse can accept a trailing escape. Delimiters expose names
    // that consume an adjacent boundary when inserted into a source form.
    let source = format!("({text})");
    let tree =
        SyntaxTree::parse_with_dialect(&source, dialect).map_err(|_| BulkRefusal::InvalidSymbol)?;
    let root = tree.root_view();
    let valid = root.children.len() == 1
        && root.children[0].children.len() == 1
        && root.children[0].children[0].kind == ExpressionKind::Atom
        && root.children[0].children[0].reader_prefixes.is_empty()
        && root.children[0].children[0].span.start().get() == 1
        && root.children[0].children[0].span.end().get() == text.len() + 1;
    if !valid {
        return Err(BulkRefusal::InvalidSymbol.into());
    }
    Ok(name)
}

fn rename_occurrences(view: &ExpressionView, from: &SymbolName, dialect: Dialect) -> Vec<ByteSpan> {
    let mut pending = vec![view];
    let mut spans = Vec::new();
    while let Some(view) = pending.pop() {
        let matches = view.kind == ExpressionKind::Atom
            && view.text.as_deref().is_some_and(|text| {
                if dialect == Dialect::CommonLisp {
                    common_lisp_symbol_reference_eq(text, from.as_str())
                } else {
                    text == from.as_str()
                }
            });
        if matches {
            spans.push(view.span);
        } else {
            pending.extend(&view.children);
        }
    }
    spans.sort_by_key(|span| span.start());
    spans
}

#[cfg(test)]
mod tests;
