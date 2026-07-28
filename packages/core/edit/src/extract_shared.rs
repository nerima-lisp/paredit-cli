use paredit_core_syntax::sexpr::{ByteSpan, Path, SyntaxTree};

use crate::error::{EditResult, InsertionRefusal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopLevelInsert {
    Append,
    Before,
    After,
}

impl TopLevelInsert {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

#[must_use]
pub fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

pub fn replace_span_checked(input: &str, span: ByteSpan, replacement: &str) -> EditResult<String> {
    span.validate_against(input)
        .map_err(|source| InsertionRefusal::InvalidReplacementSpan { source })?;
    input
        .len()
        .checked_sub(span.len())
        .and_then(|retained| retained.checked_add(replacement.len()))
        .ok_or(InsertionRefusal::ReplacementSizeOverflow)?;
    Ok(replace_span(input, span, replacement))
}

pub fn insert_top_level_form(
    input: &str,
    tree: &SyntaxTree,
    form: &str,
    insert: TopLevelInsert,
    anchor_path: Option<&Path>,
    command: &'static str,
) -> EditResult<(String, Option<ByteSpan>)> {
    match insert {
        TopLevelInsert::Append => Ok((append_top_level_form(input, form), None)),
        TopLevelInsert::Before | TopLevelInsert::After => {
            let anchor_path = anchor_path.ok_or(InsertionRefusal::MissingAnchorPath)?;
            let anchor_index = top_level_path_index(anchor_path, command)?;
            if anchor_index >= tree.root_children().len() {
                return Err(InsertionRefusal::AnchorOutOfRange {
                    anchor_path: anchor_path.to_string(),
                }
                .into());
            }
            let anchor = tree.select_path(anchor_path)?;
            let anchor_span = anchor.span();
            let (offset, inserted) = match insert {
                TopLevelInsert::Before => {
                    (anchor_span.start().get(), format!("{}\n\n", form.trim()))
                }
                TopLevelInsert::After => (anchor_span.end().get(), format!("\n\n{}", form.trim())),
                TopLevelInsert::Append => {
                    return Err(InsertionRefusal::AppendTakesNoAnchor.into());
                }
            };
            let mut output = String::with_capacity(input.len() + inserted.len());
            output.push_str(&input[..offset]);
            output.push_str(&inserted);
            output.push_str(&input[offset..]);
            Ok((output, Some(anchor_span)))
        }
    }
}

fn append_top_level_form(input: &str, form: &str) -> String {
    if input.trim().is_empty() {
        format!("{}\n", form.trim())
    } else {
        format!("{}\n\n{}\n", input.trim_end(), form.trim())
    }
}

fn top_level_path_index(path: &Path, command: &'static str) -> EditResult<usize> {
    match path.indexes() {
        [index] => Ok(index.get()),
        _ => Err(InsertionRefusal::NotTopLevelPath { command }.into()),
    }
}
